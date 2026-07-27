/// <reference lib="webworker" />
import {
  validRequest,
  PROTOCOL_VERSION,
  type CompositionChange,
  type CompositionChangeEnvelope,
  type CompositionReceipt,
  type CompositionUpdateWire,
  type Viewport,
  type WorkerMessage,
  type WorkerRequest,
} from "../browser/protocol.js";
import { commandId, linkId, nodeId } from "../core/types.js";
import {
  createInitialFxNodeComposition,
  compileFxNodeComposition,
  FxNodeCompositionError,
} from "../composition/compile.js";
import { bindFxNodeHeadless } from "../headless-runtime.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData } from "../composition/types.js";
import {
  rebindBoundEngineAuthority,
  type EngineState as BoundEngineState,
  type LoadResult as BoundLoadResult,
  type MutationEnvelope as BoundMutationEnvelope,
  type ReplayResult as BoundReplayResult,
  type SnapshotEnvelope as BoundSnapshotEnvelope,
  type TransitionResult as BoundTransitionResult,
} from "../composition/bound-engine.js";
import { applyNodeOrder, layoutGraph } from "../layout/layout-graph.js";
import { IndexedLayoutStore } from "../layout/indexed-layout-store.js";
import { viewToWorld, worldToView } from "../layout/geometry.js";
import type { LayoutSnapshot } from "../layout/types.js";
import { layoutColorPicker } from "../layout/color-picker-layout.js";
import { renderCanvas } from "../render/canvas-renderer.js";
import { paintColorPicker } from "../render/color-picker-renderer.js";
import { DirtyReason, RenderScheduler, requestViewInvalidations } from "./render-scheduler.js";
import { createSession, resetSessionForCompositionRebind, resetSessionForGraphReplacement } from "./session.js";
import { cycleEnum, numericStep, scrubValue, setNumericComponent } from "./control-edit.js";
import {
  boxNodes,
  clampResize,
  compatibleTargets,
  frameDropCandidate,
  groupRoots,
  hitTest,
  planLink,
  zoomAt,
} from "./interaction.js";
import {
  addRampMidpoint,
  addRampStop,
  distributeColorRamp,
  flipColorRamp,
  isColorRamp,
  moveRampStop,
  removeRampStop,
  setRampColor,
  type ColorRamp,
} from "../widgets/color-ramp.js";
import { appendKnifePoint, crossedLinks } from "./knife-path.js";
import { pointerLaneFence, readPointerMove } from "../browser/pointer-lane.js";
import type { PointerFence } from "../browser/protocol.js";
import { mapOklchToSrgb, maxSrgbChroma, oklabToOklch, srgbToOklab, type Rgba } from "../color/oklab.js";
import { canonicalJsonEqual, nullRecord } from "../core/json.js";
import { advanceJournal, checkpointJournal, importJournal, journalSaveData, type WorkerJournal } from "./journal.js";
import type { Command } from "../commands/types.js";
import { planSelectionMute, planSelectionRemoval } from "./selection-actions.js";
import { FXNODE_VIEW_LIMITS, fxNodeDevicePixels } from "../browser/view-limits.js";
import type { WorkerSession } from "./session.js";
import { ViewAtlasError, ViewAtlasManager } from "./view-atlas.js";

const scope = self as unknown as DedicatedWorkerGlobalScope;
let state: BoundEngineState | undefined;
let journal: WorkerJournal | undefined;
let application:
  | {
      compiled: CompiledFxNodeComposition<FxNodeCompositionData>;
      runtime: ReturnType<typeof bindFxNodeHeadless<FxNodeCompositionData>>;
    }
  | undefined;
let layoutStore: IndexedLayoutStore<FxNodeCompositionData> | undefined;
interface WorkerView {
  readonly id: string;
  readonly session: WorkerSession;
  viewport: Viewport;
  deviceWidth: number;
  deviceHeight: number;
  surfaceGeneration: number;
  resizePending: boolean;
  currentLayout: LayoutSnapshot | undefined;
  gestureLayout: LayoutSnapshot | undefined;
  readonly scheduler: RenderScheduler;
  readonly pointerLane: SharedArrayBuffer | undefined;
  handledPointerFence: number;
  consumedPointerSequence: number;
  hostGeneration: number;
  selectionSignature: string;
  inFlightPixels: { readonly frameId: number; readonly pixels: number } | undefined;
}
const views = new Map<string, WorkerView>();
let inFlightDevicePixels = 0;
const atlas = new ViewAtlasManager(undefined, (error) => fatal(error, error.code));
const usedViewIds = new Set<string>();
const resourceImages = new Map<string, { bitmap: ImageBitmap; name: string; bytes: number; lastUsed: number }>();
const resourceOperations = new Map<string, { serial: number; view: WorkerView }>();
let resourceSerial = 0;
let compositionRevision = 0;
const post = (message: WorkerMessage, transfer: Transferable[] = []): void => scope.postMessage(message, transfer);
function releaseFramePixels(view: WorkerView, frameId?: number): boolean {
  const ticket = view.inFlightPixels;
  if (!ticket || (frameId !== undefined && ticket.frameId !== frameId)) return false;
  view.inFlightPixels = undefined;
  inFlightDevicePixels -= ticket.pixels;
  return true;
}
type WireIssue = { readonly code: string; readonly path: string; readonly message: string };
type WireError = {
  readonly code: string;
  readonly message: string;
  readonly path?: string;
  readonly issues?: readonly WireIssue[];
};
const bounded = (value: unknown, limit: number, fallback: string) => String(value ?? fallback).slice(0, limit);
function toWireIssues(value: unknown): readonly WireIssue[] | undefined {
  if (!Array.isArray(value)) return;
  return value.slice(0, 100).map((issue) => {
    const item =
      typeof issue === "object" && issue !== null
        ? (issue as { code?: unknown; path?: unknown; message?: unknown })
        : {};
    return {
      code: bounded(item.code, 128, "error"),
      path: bounded(item.path, 512, "/"),
      message: bounded(item.message, 1024, "Request failed"),
    };
  });
}
function toWireError(
  value: { readonly code?: unknown; readonly message?: unknown; readonly path?: unknown; readonly issues?: unknown },
  fallbackCode = "worker.error",
  fallbackMessage = "Request failed",
): WireError {
  const path = value.path === undefined ? undefined : bounded(value.path, 512, "/");
  const issues = toWireIssues(value.issues);
  return {
    code: bounded(value.code, 128, fallbackCode),
    message: bounded(value.message, 2048, fallbackMessage),
    ...(path === undefined ? {} : { path }),
    ...(issues === undefined ? {} : { issues }),
  };
}
function reject(
  id: string,
  error: { readonly code?: unknown; readonly message?: unknown; readonly path?: unknown; readonly issues?: unknown },
): void {
  post({ protocol: PROTOCOL_VERSION, type: "response", id, ok: false, error: toWireError(error) });
}

async function drawView(view: WorkerView, frameId: number, renderId: number, reasons: number): Promise<void> {
  if (!state || views.get(view.id) !== view || view.resizePending) {
    if (views.get(view.id) === view) view.scheduler.defer(frameId, reasons);
    return;
  }
  const pixels = view.deviceWidth * view.deviceHeight;
  if (view.inFlightPixels || inFlightDevicePixels + pixels > FXNODE_VIEW_LIMITS.maxInFlightDevicePixels) {
    view.scheduler.defer(frameId, reasons);
    return;
  }
  view.inFlightPixels = { frameId, pixels };
  inFlightDevicePixels += pixels;
  advanceCollapseAnimations(view, performance.now());
  const expectedWidth = view.deviceWidth,
    expectedHeight = view.deviceHeight;
  const preview =
    view.session.previewPositions.size || view.session.previewSizes.size || view.session.previewValues.size;
  const nodes = preview
    ? Object.fromEntries(
        Object.entries(state.document.nodes).map(([id, node]) => {
          const values = [...view.session.previewValues].filter(([controlId]) => controlId.startsWith(`${id}:`));
          let next = {
            ...node,
            ...(view.session.previewPositions.has(node.id)
              ? { position: view.session.previewPositions.get(node.id)! }
              : {}),
            ...(view.session.previewSizes.has(node.id) ? { size: view.session.previewSizes.get(node.id)! } : {}),
          };
          for (const [controlId, value] of values) {
            const control = view.currentLayout?.controls.get(controlId);
            if (control?.source === "parameter" && next.known)
              next = { ...next, parameters: { ...next.parameters, [control.key]: value } };
            else if (control?.source === "socket-default")
              next = {
                ...next,
                sockets: next.sockets.map((socket) =>
                  socket.id === control.key ? { ...socket, defaultValue: value } : socket,
                ),
              };
          }
          return [id, next];
        }),
      )
    : state.document.nodes;
  const transform = currentTransform(view);
  const layout = applyNodeOrder(
    nodes === state.document.nodes && layoutStore
      ? layoutStore.view(transform)
      : layoutGraph(application!.compiled, { ...state.document, nodes }, transform),
    view.session.uiOrder,
  );
  view.currentLayout = layout;
  publishSelection(view);
  if (view.session.collapseAnimations.size) view.scheduler.request(renderId, DirtyReason.Preview);
  let frameHostGeneration = 0,
    frameSurfaceGeneration = 0,
    frameDeviceWidth = 0,
    frameDeviceHeight = 0,
    frameHost = { colorPickerOpen: false };
  const bitmap = await atlas.renderAndCrop(view.id, { width: expectedWidth, height: expectedHeight }, (target) => {
    frameHostGeneration = view.hostGeneration;
    frameSurfaceGeneration = view.surfaceGeneration;
    frameDeviceWidth = view.deviceWidth;
    frameDeviceHeight = view.deviceHeight;
    frameHost = { colorPickerOpen: !!view.session.colorPicker };
    const deviceTarget = {
      x: target.deviceX,
      y: target.deviceY,
      width: target.deviceWidth,
      height: target.deviceHeight,
    };
    renderCanvas(target.context, layout, application!.compiled.theme, view.session, resourceImages, deviceTarget);
    if (view.session.colorPicker)
      paintColorPicker(
        target.context,
        view.session.colorPicker.layout,
        view.session.colorPicker.model,
        view.session.colorPicker.rgba,
        view.session.colorPicker.hsv,
        view.session.colorPicker.edit,
        view.viewport.dpr,
        deviceTarget,
      );
  });
  if (!bitmap) {
    releaseFramePixels(view, frameId);
    if (views.get(view.id) === view) view.scheduler.defer(frameId, reasons);
    return;
  }
  if (views.get(view.id) !== view) {
    bitmap.close();
    releaseFramePixels(view, frameId);
    return;
  }
  try {
    post(
      {
        protocol: PROTOCOL_VERSION,
        type: "view.frame",
        viewId: view.id,
        bitmap,
        renderId,
        frameId,
        hostGeneration: frameHostGeneration,
        surfaceGeneration: frameSurfaceGeneration,
        deviceWidth: frameDeviceWidth,
        deviceHeight: frameDeviceHeight,
        host: frameHost,
      },
      [bitmap],
    );
  } catch (error) {
    bitmap.close();
    releaseFramePixels(view, frameId);
    throw error;
  }
}
function fatal(error: unknown, code = "worker.fatal"): void {
  if (fataled) return;
  fataled = true;
  for (const view of views.values()) {
    view.scheduler.stop();
    releaseFramePixels(view);
  }
  views.clear();
  atlas.dispose();
  resourceOperations.clear();
  closeResourceImages();
  post({
    protocol: PROTOCOL_VERSION,
    type: "fatal",
    error: { code, message: error instanceof Error ? error.message : "Worker failure" },
  });
  scope.close();
}
let fataled = false;
type CommitPresentationPolicy = { readonly kind: "none" } | { readonly kind: "select-added"; readonly nodeId: string };
const noPresentation: CommitPresentationPolicy = { kind: "none" };
function commit(
  result: Extract<BoundTransitionResult, { status: "committed" }>,
  command: Command,
  sourceView?: WorkerView,
  presentation: CommitPresentationPolicy = noPresentation,
): void {
  for (const view of views.values()) cancelGesture(view);
  const nextJournal = advanceJournal(
    journal!,
    command,
    application!.runtime.save(result.state.document),
    (baseline, commands) => application!.runtime.validateReplayJournal(baseline, commands, result.state.historyLimit),
  );
  if (result.mutationEnvelope.mutations.some((mutation) => mutation.kind === "document.replaced")) {
    installDocumentReplacement(result, nextJournal);
    return;
  }
  state = result.state;
  journal = nextJournal;
  const collapseTimestamp = performance.now();
  for (const view of views.values()) seedCollapseAnimations(view, result.mutationEnvelope.mutations, collapseTimestamp);
  layoutStore?.rebuild(result.state.document);
  for (const view of views.values()) pruneSession(view);
  if (
    presentation.kind === "select-added" &&
    sourceView &&
    views.get(sourceView.id) === sourceView &&
    state.document.nodes[presentation.nodeId]
  ) {
    const id = nodeId(presentation.nodeId);
    sourceView.session.selectedNodes = new Set([id]);
    sourceView.session.selectedLinks.clear();
    sourceView.session.activeNode = id;
    raiseNode(sourceView, id);
  }
  post({ protocol: PROTOCOL_VERSION, type: "mutation", envelope: result.mutationEnvelope });
  post({ protocol: PROTOCOL_VERSION, type: "snapshot.event", envelope: result.snapshotEnvelope });
  for (const view of views.values()) {
    refreshLayout(view);
    publishSelection(view);
    view.scheduler.request();
  }
}
function installDocumentReplacement(
  result:
    | Extract<BoundTransitionResult, { status: "committed" }>
    | Extract<BoundLoadResult, { ok: true }>
    | Extract<BoundReplayResult, { ok: true; status: "committed" }>,
  nextJournal: WorkerJournal,
): void {
  const nextStore = new IndexedLayoutStore(application!.compiled, result.state.document);
  for (const view of views.values()) cancelGesture(view);
  state = result.state;
  journal = nextJournal;
  layoutStore = nextStore;
  for (const view of views.values()) {
    view.gestureLayout = undefined;
    resetSessionForGraphReplacement(view.session);
    view.currentLayout = applyNodeOrder(nextStore.view(currentTransform(view)), []);
  }
  closeResourceImages();
  post({ protocol: PROTOCOL_VERSION, type: "mutation", envelope: result.mutationEnvelope });
  post({ protocol: PROTOCOL_VERSION, type: "snapshot.event", envelope: result.snapshotEnvelope });
  for (const view of views.values()) {
    publishSelection(view);
    view.scheduler.request();
  }
}
function collapseValue(animation: import("./session.js").CollapseAnimation, now: number): number {
  const t = Math.max(0, Math.min(1, (now - animation.startedAt) / animation.durationMs)),
    eased = 1 - (1 - t) ** 3;
  return animation.from + (animation.to - animation.from) * eased;
}
function advanceCollapseAnimations(view: WorkerView, now: number): void {
  for (const [id, animation] of view.session.collapseAnimations) {
    animation.value = collapseValue(animation, now);
    if (now - animation.startedAt >= animation.durationMs) view.session.collapseAnimations.delete(id);
  }
}
function seedCollapseAnimations(
  view: WorkerView,
  mutations: readonly import("../engine/mutations.js").Mutation[],
  now: number,
): void {
  const changes = new Map<
    import("../core/types.js").NodeId,
    { before: import("../core/types.js").GraphNode | null; after: import("../core/types.js").GraphNode | null }
  >();
  for (const mutation of mutations)
    if (mutation.kind === "node.set") {
      const prior = changes.get(mutation.id);
      changes.set(mutation.id, { before: prior ? prior.before : mutation.before, after: mutation.after });
    }
  for (const [id, change] of changes) {
    if (!change.after) {
      view.session.collapseAnimations.delete(id);
      continue;
    }
    if (!change.before || change.before.collapsed === change.after.collapsed) continue;
    const existing = view.session.collapseAnimations.get(id),
      from = existing ? collapseValue(existing, now) : change.before.collapsed ? 1 : 0,
      to = change.after.collapsed ? 1 : 0,
      distance = Math.abs(to - from);
    if (distance < 0.001) {
      view.session.collapseAnimations.delete(id);
      continue;
    }
    view.session.collapseAnimations.set(id, { from, to, value: from, startedAt: now, durationMs: 120 * distance });
  }
}
function cancelGesture(view: WorkerView): boolean {
  const active =
    !!view.session.knife ||
    !!view.session.drag ||
    !!view.session.scrub ||
    !!view.session.rampDrag ||
    !!view.session.reroutePress ||
    !!view.session.modalMove ||
    !!view.session.box ||
    !!view.session.linkDrag ||
    !!view.session.resize ||
    !!view.session.pan ||
    !!view.session.controlEdit ||
    !!view.session.colorPicker ||
    !!view.session.colorWheel ||
    !!view.gestureLayout ||
    view.session.previewValues.size > 0 ||
    view.session.previewPositions.size > 0 ||
    view.session.previewSizes.size > 0;
  if (view.session.box) view.session.selectedNodes = new Set(view.session.box.checkpoint);
  delete view.session.knife;
  delete view.session.drag;
  delete view.session.scrub;
  delete view.session.rampDrag;
  delete view.session.reroutePress;
  delete view.session.modalMove;
  delete view.session.box;
  delete view.session.linkDrag;
  delete view.session.resize;
  delete view.session.parentHighlight;
  delete view.session.pan;
  delete view.session.controlEdit;
  delete view.session.colorPicker;
  delete view.session.colorWheel;
  view.session.previewValues.clear();
  view.gestureLayout = undefined;
  view.session.previewPositions.clear();
  view.session.previewSizes.clear();
  return active;
}
const currentTransform = (view: WorkerView) => ({
  center: view.session.cameraCenter,
  zoom: view.session.zoom,
  viewport: { x: view.viewport.width, y: view.viewport.height },
  dpr: view.viewport.dpr,
});
function raiseNode(view: WorkerView, id: import("../core/types.js").NodeId): void {
  view.session.uiOrder = [...view.session.uiOrder.filter((value) => value !== id), id];
}
function selectNode(view: WorkerView, id: import("../core/types.js").NodeId, add = false): void {
  if (add) {
    view.session.selectedNodes.has(id) ? view.session.selectedNodes.delete(id) : view.session.selectedNodes.add(id);
  } else {
    view.session.selectedNodes.clear();
    view.session.selectedNodes.add(id);
  }
  view.session.selectedLinks.clear();
  view.session.activeNode = id;
  raiseNode(view, id);
}
function refreshLayout(view: WorkerView): void {
  if (layoutStore) view.currentLayout = applyNodeOrder(layoutStore.view(currentTransform(view)), view.session.uiOrder);
}
function pruneSession(view: WorkerView): void {
  if (!state) return;
  const ids = new Set(Object.keys(state.document.nodes)),
    links = new Set(Object.keys(state.document.links));
  view.session.selectedNodes = new Set([...view.session.selectedNodes].filter((id) => ids.has(id)));
  view.session.selectedLinks = new Set([...view.session.selectedLinks].filter((id) => links.has(id)));
  view.session.uiOrder = view.session.uiOrder.filter((id) => ids.has(id));
  if (view.session.activeNode && !ids.has(view.session.activeNode)) delete view.session.activeNode;
}
function isStandardNode(id: import("../core/types.js").NodeId): boolean {
  const node = state?.document.nodes[id];
  return !!node?.known && application?.compiled.nodes.get(node.typeId)?.behavior === "standard";
}
function publishSelection(view: WorkerView, force = false): void {
  if (!state || !application) return;
  const nodes = [...view.session.selectedNodes]
      .map((id) => state!.document.nodes[id])
      .filter((node): node is NonNullable<typeof node> => !!node),
    linkCount = [...view.session.selectedLinks].filter((id) => state!.document.links[id]).length,
    eligible = nodes.filter((node) => isStandardNode(node.id)),
    mute = eligible.length
      ? {
          enabled: true as const,
          state: (eligible.every((node) => node.muted)
            ? "all-muted"
            : eligible.every((node) => !node.muted)
              ? "all-unmuted"
              : "mixed") as "all-muted" | "all-unmuted" | "mixed",
        }
      : { enabled: false as const },
    projection = { nodeCount: nodes.length, linkCount, canRemove: nodes.length + linkCount > 0, mute },
    signature = `${projection.nodeCount}|${projection.linkCount}|${+projection.canRemove}|${mute.enabled ? mute.state : "disabled"}`;
  if (!force && signature === view.selectionSignature) return;
  view.selectionSignature = signature;
  post({ protocol: PROTOCOL_VERSION, type: "view.selection.host", viewId: view.id, projection });
}
function controlCommand(
  view: WorkerView,
  id: string,
  value?: import("../core/types.js").ParameterValue,
  reset = false,
): import("../commands/types.js").Command | undefined {
  const control = view.currentLayout?.controls.get(id);
  if (!control || control.source === "unknown") return;
  if (control.source === "parameter")
    return reset
      ? { type: "node.parameter-reset", id: control.nodeId, key: control.key }
      : { type: "node.parameter", id: control.nodeId, key: control.key, value: value! };
  return reset
    ? {
        type: "node.socket-default-reset",
        id: control.nodeId,
        socketId: control.key as import("../core/types.js").SocketId,
      }
    : {
        type: "node.socket-default",
        id: control.nodeId,
        socketId: control.key as import("../core/types.js").SocketId,
        value: value!,
      };
}
function gestureCommand(view: WorkerView, command: import("../commands/types.js").Command): void {
  if (!state) return;
  const result = application!.runtime.transition(state, {
    commandId: commandId(`gesture-${state.version + 1}`),
    expectedVersion: state.version,
    source: "gesture",
    command,
  });
  if (result.status === "committed") commit(result, command, view);
}
const rampValue = (view: WorkerView, id: string, layout = view.currentLayout) => {
  const c = layout?.controls.get(id),
    v = c?.value as { kind?: unknown; value?: unknown };
  return c?.kind === "color-ramp" && c.rampBounds && v?.kind === "json" && isColorRamp(v.value)
    ? { control: c, ramp: v.value }
    : undefined;
};
const activeStop = (view: WorkerView, id: string, ramp: ColorRamp) => {
  const stored = view.session.activeRampStopByControl.get(id);
  return ramp.stops.find((s) => s.id === stored) ?? ramp.stops[0]!;
};
function commitRamp(view: WorkerView, id: string, ramp: ColorRamp, active?: string) {
  const c = view.currentLayout?.controls.get(id);
  if (!c) return;
  if (active) view.session.activeRampStopByControl.set(id, active);
  const command = controlCommand(view, id, {
    kind: "json",
    value: ramp as unknown as import("../core/types.js").JsonValue,
  });
  if (command) gestureCommand(view, command);
}
function rampAction(view: WorkerView, id: string, target: string, position?: number): void {
  const found = rampValue(view, id);
  if (!found) return;
  let { ramp } = found;
  const active = activeStop(view, id, ramp);
  if (target === "add") {
    const newId = `stop-${state!.version + 1}-${ramp.stops.length}`,
      next = addRampMidpoint(ramp, active.id, newId);
    commitRamp(view, id, next, next === ramp ? active.id : newId);
    return;
  }
  if (target === "remove") {
    const next = removeRampStop(ramp, active.id),
      survivor = next.stops.reduce((a, b) =>
        Math.abs(b.position - active.position) < Math.abs(a.position - active.position) ? b : a,
      );
    commitRamp(view, id, next, survivor.id);
    return;
  }
  if (target === "flip") ramp = flipColorRamp(ramp);
  else if (target === "distribute") ramp = distributeColorRamp(ramp);
  else if (target === "mode") {
    const options = ["rgb", "hsv", "hsl"] as const;
    ramp = { ...ramp, colorMode: options[(options.indexOf(ramp.colorMode) + 1) % options.length]! };
  } else if (target === "interpolation") {
    const options = ["linear", "ease", "constant", "cardinal", "b-spline"] as const;
    ramp = { ...ramp, interpolation: options[(options.indexOf(ramp.interpolation) + 1) % options.length]! };
  } else if (target === "hue") {
    const options = ["near", "far", "clockwise", "counter-clockwise"] as const;
    ramp = { ...ramp, hueInterpolation: options[(options.indexOf(ramp.hueInterpolation) + 1) % options.length]! };
  } else if (target === "gradient" && position !== undefined) {
    const newId = `stop-${state!.version + 1}-${ramp.stops.length}`,
      next = addRampStop(ramp, position, newId);
    commitRamp(view, id, next, next === ramp ? active.id : newId);
    return;
  }
  commitRamp(view, id, ramp, active.id);
}
const containsView = (point: { x: number; y: number }, rect: { x: number; y: number; width: number; height: number }) =>
  point.x >= rect.x && point.x <= rect.x + rect.width && point.y >= rect.y && point.y <= rect.y + rect.height;
function rgbToHsv(rgba: Rgba, oldHue = 0): readonly [number, number, number] {
  const [r, g, b] = rgba,
    max = Math.max(r, g, b),
    min = Math.min(r, g, b),
    d = max - min,
    s = max ? d / max : 0;
  let h = oldHue;
  if (d) h = 60 * (max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4);
  return [((h % 360) + 360) % 360, s, max];
}
function hsvToRgb(h: number, s: number, v: number, a: number): Rgba {
  h = ((h % 360) + 360) % 360;
  s = Math.max(0, Math.min(1, s));
  v = Math.max(0, Math.min(1, v));
  const c = v * s,
    x = c * (1 - Math.abs(((h / 60) % 2) - 1)),
    m = v - c,
    [r, g, b] =
      h < 60
        ? [c, x, 0]
        : h < 120
          ? [x, c, 0]
          : h < 180
            ? [0, c, x]
            : h < 240
              ? [0, x, c]
              : h < 300
                ? [x, 0, c]
                : [c, 0, x];
  return [r + m, g + m, b + m, a];
}
function beginColorPicker(
  view: WorkerView,
  id: string,
  anchor: { x: number; y: number; width: number; height: number },
  rgba: Rgba,
  target: { kind: "control" } | { kind: "ramp-stop"; stopId: string; original: ColorRamp },
): void {
  const model = oklabToOklch(srgbToOklab([rgba[0], rgba[1], rgba[2]]));
  view.session.colorPicker = {
    layout: layoutColorPicker(anchor, { x: view.viewport.width, y: view.viewport.height }),
    controlId: id,
    target,
    model,
    rgba,
    hsv: rgbToHsv(rgba),
  };
  publishPicker(view);
  view.scheduler.request();
}
function publishPicker(view: WorkerView): void {
  const p = view.session.colorPicker;
  if (!p) return;
  p.model = oklabToOklch(srgbToOklab([p.rgba[0], p.rgba[1], p.rgba[2]]), p.model.h);
  if (p.target.kind === "control") view.session.previewValues.set(p.controlId, { kind: "color", value: p.rgba });
  else
    view.session.previewValues.set(p.controlId, {
      kind: "json",
      value: setRampColor(
        p.target.original,
        p.target.stopId,
        p.rgba,
      ) as unknown as import("../core/types.js").JsonValue,
    });
  view.scheduler.request();
}
function applyPickerEdit(view: WorkerView, keepInvalid = true): boolean {
  const p = view.session.colorPicker,
    e = p?.edit;
  if (!p || !e) return true;
  let rgba: Rgba | undefined;
  if (e.field === "hex") {
    const match = e.buffer.trim().match(/^#?([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i),
      raw = match?.[1];
    if (raw) {
      const expanded = raw.length < 5 ? [...raw].map((value) => value + value).join("") : raw,
        full = expanded.length === 6 ? expanded + "ff" : expanded;
      rgba = [0, 2, 4, 6].map((index) => parseInt(full.slice(index, index + 2), 16) / 255) as unknown as Rgba;
    }
  } else {
    const value = Number(e.buffer);
    if (Number.isFinite(value)) {
      if (e.field === "rgba") {
        const next = [...p.rgba] as number[];
        next[e.index] = Math.max(0, Math.min(1, value));
        rgba = next as unknown as Rgba;
      } else {
        const next = [...p.hsv] as number[];
        next[e.index] = e.index ? Math.max(0, Math.min(1, value)) : ((value % 360) + 360) % 360;
        p.hsv = next as unknown as typeof p.hsv;
        rgba = hsvToRgb(next[0]!, next[1]!, next[2]!, p.rgba[3]);
      }
    }
  }
  if (!rgba) {
    if (keepInvalid) {
      p.edit = { ...e, invalid: true };
      view.scheduler.request();
    } else delete p.edit;
    return false;
  }
  p.rgba = rgba;
  p.hsv = rgbToHsv(rgba, p.hsv[0]);
  delete p.edit;
  publishPicker(view);
  return true;
}
function openRampPicker(view: WorkerView, id: string, ramp: ColorRamp): void {
  const control = view.currentLayout?.controls.get(id),
    bounds = control?.rampBounds;
  if (!control || !bounds) return;
  const stop = activeStop(view, id, ramp),
    anchor = viewRectFromWorld(bounds.color, view.currentLayout!);
  beginColorPicker(view, id, anchor, stop.color as Rgba, { kind: "ramp-stop", stopId: stop.id, original: ramp });
}
function openControlPicker(view: WorkerView, id: string): void {
  const control = view.currentLayout?.controls.get(id),
    value = control?.value as import("../core/types.js").ParameterValue | undefined;
  if (!control || control.kind !== "color" || control.linked || control.source === "unknown" || value?.kind !== "color")
    return;
  beginColorPicker(view, id, viewRectFromWorld(control.bounds, view.currentLayout!), value.value as Rgba, {
    kind: "control",
  });
}
function updatePicker(view: WorkerView, position: { x: number; y: number }): void {
  const picker = view.session.colorPicker;
  if (!picker?.drag) return;
  const rect = picker.layout[picker.drag.region];
  if (picker.drag.region === "plane") {
    const radius = rect.width / 2,
      dx = (position.x - (rect.x + radius)) / radius,
      dy = (rect.y + radius - position.y) / radius,
      length = Math.min(1, Math.hypot(dx, dy)),
      h = Math.atan2(dy, dx),
      c = length * maxSrgbChroma(picker.model.l, h);
    picker.model = { ...picker.model, c, h };
  } else if (picker.drag.region === "lightness") {
    const l = 1 - Math.max(0, Math.min(1, (position.y - rect.y) / rect.height));
    picker.model = { ...picker.model, l, c: Math.min(picker.model.c, maxSrgbChroma(l, picker.model.h)) };
  } else
    picker.rgba = [
      picker.rgba[0],
      picker.rgba[1],
      picker.rgba[2],
      1 - Math.max(0, Math.min(1, (position.y - rect.y) / rect.height)),
    ];
  const rgb = mapOklchToSrgb(picker.model);
  picker.rgba = [rgb[0], rgb[1], rgb[2], picker.rgba[3]];
  picker.hsv = rgbToHsv(picker.rgba, picker.hsv[0]);
  publishPicker(view);
}
function finishPicker(view: WorkerView, commitValue: boolean): void {
  const picker = view.session.colorPicker;
  if (!picker) return;
  if (commitValue) applyPickerEdit(view, false);
  const preview = view.session.previewValues.get(picker.controlId);
  view.session.previewValues.delete(picker.controlId);
  delete view.session.colorPicker;
  if (commitValue && preview) {
    if (picker.target.kind === "control") {
      const command = controlCommand(view, picker.controlId, preview);
      if (command) gestureCommand(view, command);
    } else {
      const value = preview as { kind?: unknown; value?: unknown };
      if (value.kind === "json" && isColorRamp(value.value))
        commitRamp(view, picker.controlId, value.value, picker.target.stopId);
    }
  }
  view.scheduler.request();
}
const viewRectFromWorld = (rect: { x: number; y: number; width: number; height: number }, layout: LayoutSnapshot) => {
  const p = worldToView({ x: rect.x, y: rect.y }, layout.transform);
  return { x: p.x, y: p.y, width: rect.width * layout.transform.zoom, height: rect.height * layout.transform.zoom };
};
function updateInlineWheel(view: WorkerView, position: { x: number; y: number }): void {
  const wheel = view.session.colorWheel;
  if (!wheel) return;
  const { bounds } = wheel;
  if (wheel.region === "plane") {
    const radius = bounds.width / 2,
      dx = (position.x - (bounds.x + radius)) / radius,
      dy = (bounds.y + radius - position.y) / radius,
      length = Math.min(1, Math.hypot(dx, dy)),
      h = Math.atan2(dy, dx),
      l = Math.min(0.9, Math.max(0.1, wheel.model.l));
    wheel.model = { l, h, c: length * maxSrgbChroma(l, h) };
  } else {
    const l = 1 - Math.max(0, Math.min(1, (position.y - bounds.y) / bounds.height));
    wheel.model = { ...wheel.model, l, c: Math.min(wheel.model.c, maxSrgbChroma(l, wheel.model.h)) };
  }
  const rgb = mapOklchToSrgb(wheel.model);
  wheel.rgba = [rgb[0], rgb[1], rgb[2], wheel.rgba[3]];
  view.session.previewValues.set(wheel.controlId, { kind: "color", value: wheel.rgba });
  view.scheduler.request();
}
function finishInlineWheel(view: WorkerView, commitValue: boolean): void {
  const wheel = view.session.colorWheel;
  if (!wheel) return;
  const preview = view.session.previewValues.get(wheel.controlId);
  view.session.previewValues.delete(wheel.controlId);
  delete view.session.colorWheel;
  view.gestureLayout = undefined;
  if (commitValue && preview) {
    const command = controlCommand(view, wheel.controlId, preview);
    if (command) gestureCommand(view, command);
  } else view.scheduler.request();
}
function processInput(
  view: WorkerView,
  event: Extract<import("../browser/protocol.js").WorkerRequest, { type: "view.input" }>["event"],
  nodeMenuRequestId?: string,
  resourceOpenRequestId?: string,
): void {
  if (!state) return;
  if (!view.currentLayout && layoutStore) view.currentLayout = layoutStore.view(currentTransform(view));
  if (!view.currentLayout) return;
  if (event.kind === "outside-pointer") {
    if (event.button === 0 && view.session.colorPicker) finishPicker(view, true);
    return;
  }
  if (event.kind === "focus" && event.phase === "blur") delete view.session.knife;
  if (event.kind === "key" && event.phase === "down" && event.key === "Escape") delete view.session.knife;
  if (event.kind === "focus") {
    if (event.phase === "blur") {
      cancelGesture(view);
      view.scheduler.request();
    }
    return;
  }
  if (event.kind === "wheel") {
    if (view.session.colorPicker) return;
    const next = zoomAt(currentTransform(view), event.position, event.delta.y);
    view.session.zoom = next.zoom;
    view.session.cameraCenter = next.center;
    refreshLayout(view);
    view.scheduler.request();
    return;
  }
  if (event.kind === "key" && event.phase === "down") {
    const modifier = (event.modifiers & 6) !== 0;
    if (event.key === "Escape") {
      cancelGesture(view);
      view.scheduler.request();
      return;
    }
    if (view.session.colorPicker) {
      const p = view.session.colorPicker,
        e = p.edit;
      if (e) {
        if (event.key === "Enter") {
          applyPickerEdit(view);
          return;
        }
        if (event.key === "Backspace") {
          p.edit = { ...e, buffer: e.selectAll ? "" : e.buffer.slice(0, -1), selectAll: false, invalid: false };
          view.scheduler.request();
          return;
        }
        if (event.key.length === 1 && !modifier) {
          p.edit = { ...e, buffer: e.selectAll ? event.key : e.buffer + event.key, selectAll: false, invalid: false };
          view.scheduler.request();
        }
        return;
      }
      return;
    }
    if (event.key === "Backspace" && view.session.controlEdit) {
      view.session.controlEdit = {
        ...view.session.controlEdit,
        buffer:
          view.session.controlEdit.kind === "number" && view.session.controlEdit.selectAll
            ? ""
            : view.session.controlEdit.buffer.slice(0, -1),
        ...(view.session.controlEdit.kind === "number" ? { selectAll: false } : {}),
      };
      view.scheduler.request();
      return;
    }
    if (event.key === "Backspace") {
      if (view.session.colorPicker) finishPicker(view, false);
      const id = view.session.focusedControl ?? view.session.hoveredControl;
      if (id) {
        const rv = rampValue(view, id);
        if (rv) {
          const stop = activeStop(view, id, rv.ramp),
            target = view.session.focusedRampTarget;
          if (target === "swatch") commitRamp(view, id, setRampColor(rv.ramp, stop.id, [0, 0, 0, 1]), stop.id);
          else {
            const command = controlCommand(view, id, undefined, true);
            if (command) gestureCommand(view, command);
          }
          return;
        }
        const command = controlCommand(view, id, undefined, true);
        if (command) gestureCommand(view, command);
      }
      return;
    }
    if (view.session.focusedControl && view.session.focusedRampTarget) {
      const rv = rampValue(view, view.session.focusedControl);
      if (rv) {
        const stop = activeStop(view, view.session.focusedControl, rv.ramp);
        if (event.key === "Delete" || event.key.toLowerCase() === "x") {
          rampAction(view, view.session.focusedControl, "remove");
          return;
        }
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          const step = (event.modifiers & 8) !== 0 ? 0.001 : 0.01;
          commitRamp(
            view,
            view.session.focusedControl,
            moveRampStop(rv.ramp, stop.id, stop.position + (event.key === "ArrowRight" ? step : -step)),
            stop.id,
          );
          return;
        }
        return;
      }
    }
    if (view.session.focusedControl && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
      const control = view.currentLayout.controls.get(view.session.focusedControl);
      const value = control?.value;
      if (
        control?.kind === "enum" &&
        control.schema?.type === "string" &&
        control.schema.enum &&
        value &&
        typeof value === "object" &&
        "kind" in value &&
        value.kind === "string" &&
        "value" in value &&
        typeof value.value === "string"
      ) {
        const next = cycleEnum(control.schema.enum, value.value, event.key === "ArrowDown" ? 1 : -1);
        const command = controlCommand(view, control.id, { kind: "string", value: next });
        if (command) gestureCommand(view, command);
      }
      return;
    }
    if (view.session.controlEdit?.kind === "string") {
      if (event.key === "Enter") {
        const edit = view.session.controlEdit;
        delete view.session.controlEdit;
        const command = controlCommand(view, edit.controlId, { kind: "string", value: edit.buffer });
        if (command) gestureCommand(view, command);
        return;
      }
      if (event.key.length === 1 && !modifier) {
        view.session.controlEdit = { ...view.session.controlEdit, buffer: view.session.controlEdit.buffer + event.key };
        view.scheduler.request();
        return;
      }
    }
    if (view.session.controlEdit?.kind === "number") {
      if (event.key === "Enter") {
        const edit = view.session.controlEdit,
          value = Number(edit.buffer),
          control = view.currentLayout.controls.get(edit.controlId),
          original = control?.value as import("../core/types.js").ParameterValue | undefined;
        if (Number.isFinite(value) && control && original) {
          delete view.session.controlEdit;
          const command = controlCommand(
            view,
            edit.controlId,
            setNumericComponent(control, original, edit.component, value),
          );
          if (command) gestureCommand(view, command);
        }
        return;
      }
      if (event.key.length === 1 && !modifier && /[0-9eE+.-]/.test(event.key)) {
        view.session.controlEdit = {
          ...view.session.controlEdit,
          buffer: view.session.controlEdit.selectAll ? event.key : view.session.controlEdit.buffer + event.key,
          selectAll: false,
        };
        view.scheduler.request();
        return;
      }
    }
    if (modifier && event.key.toLowerCase() === "z") {
      gestureCommand(view, { type: (event.modifiers & 8) !== 0 ? "redo" : "undo" });
      return;
    }
    if (event.key === "Home") {
      const b = view.currentLayout.graphBounds;
      if (b.width || b.height) {
        view.session.cameraCenter = { x: b.x + b.width / 2, y: b.y - b.height / 2 };
        view.session.zoom = Math.min(
          2,
          Math.max(0.1, Math.min(view.viewport.width / (b.width + 80), view.viewport.height / (b.height + 80))),
        );
        view.scheduler.request();
      }
      return;
    }
    const ids = [...view.session.selectedNodes];
    if (event.key.toLowerCase() === "g" && ids.length && !view.session.modalMove) {
      const startView = view.session.pointer ?? { x: view.viewport.width / 2, y: view.viewport.height / 2 },
        roots = groupRoots(view.session.selectedNodes, view.currentLayout);
      view.gestureLayout = view.currentLayout;
      view.session.modalMove = {
        startView,
        startWorld: viewToWorld(startView, view.currentLayout.transform),
        origins: new Map(roots.map((id) => [id, state!.document.nodes[id]!.position])),
        moved: true,
      };
      return;
    }
    if (event.key === "Enter" && view.session.modalMove) {
      finishMove(view);
      return;
    }
    if (event.key === "Delete" || event.key.toLowerCase() === "x") {
      const command = planSelectionRemoval(state.document, view.session.selectedNodes, view.session.selectedLinks);
      if (command) gestureCommand(view, command);
    } else if (event.key.toLowerCase() === "m" && ids.length) {
      const command = planSelectionMute(state.document, view.session.selectedNodes, isStandardNode);
      if (command) gestureCommand(view, command);
    } else if (event.key.toLowerCase() === "h" && ids.length)
      gestureCommand(view, {
        type: "batch",
        commands: ids.map((id) => ({ type: "node.collapse", id, value: !state!.document.nodes[id]?.collapsed })),
      });
    return;
  }
  if (event.kind !== "pointer") return;
  view.session.pointer = event.position;
  if (view.session.colorPicker) {
    const picker = view.session.colorPicker;
    if (event.phase === "down" && event.button === 2) {
      finishPicker(view, false);
      return;
    }
    if (picker.drag?.pointerId === event.pointerId) {
      if (event.phase === "move") updatePicker(view, event.position);
      else {
        delete picker.drag;
        view.scheduler.request();
      }
      return;
    }
    if (event.phase === "down" && event.button === 0) {
      if (containsView(event.position, picker.layout.confirm)) {
        finishPicker(view, true);
        return;
      }
      if (picker.edit) applyPickerEdit(view, false);
      const region = (["plane", "lightness", "alpha"] as const).find((name) =>
        containsView(event.position, picker.layout[name]),
      );
      if (region) {
        picker.drag = { pointerId: event.pointerId, region };
        updatePicker(view, event.position);
        return;
      }
      for (const name of ["rgba", "hsv"] as const) {
        const index = picker.layout[name].findIndex((r) => containsView(event.position, r));
        if (index >= 0) {
          const values = name === "rgba" ? picker.rgba : picker.hsv;
          picker.edit = {
            field: name,
            index,
            buffer: name === "hsv" && index === 0 ? values[index]!.toFixed(1) : values[index]!.toFixed(3),
            selectAll: true,
            invalid: false,
          };
          view.scheduler.request();
          return;
        }
      }
      if (containsView(event.position, picker.layout.hex)) {
        picker.edit = {
          field: "hex",
          index: 0,
          buffer:
            "#" +
            picker.rgba
              .map((v) =>
                Math.round(v * 255)
                  .toString(16)
                  .padStart(2, "0"),
              )
              .join("")
              .toUpperCase(),
          selectAll: true,
          invalid: false,
        };
        view.scheduler.request();
        return;
      }
      finishPicker(view, true);
      return;
    }
    return;
  }
  if (view.session.colorWheel?.pointerId === event.pointerId) {
    if (event.phase === "move") updateInlineWheel(view, event.position);
    else finishInlineWheel(view, event.phase === "up");
    return;
  }
  if (event.phase === "cancel") delete view.session.knife;
  if (event.phase === "down" && event.button === 2 && (event.modifiers & 2) !== 0) {
    cancelGesture(view);
    view.gestureLayout = view.currentLayout;
    view.session.knife = {
      pointerId: event.pointerId,
      points: [event.position],
      crossed: new Set(),
      mode: (event.modifiers & 1) !== 0 ? "mute" : "remove",
    };
    view.scheduler.request();
    return;
  }
  if (view.session.knife?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      view.session.knife.points = appendKnifePoint(view.session.knife.points, event.position);
      view.session.knife.crossed = crossedLinks(
        view.gestureLayout!,
        view.session.knife.points,
        view.session.knife.mode === "mute",
      );
      view.scheduler.request();
      return;
    }
    if (event.phase === "up") {
      const knife = view.session.knife;
      delete view.session.knife;
      view.gestureLayout = undefined;
      const commands = [...knife.crossed].map((id) =>
        knife.mode === "remove"
          ? { type: "link.remove" as const, id }
          : { type: "link.mute" as const, id, value: !state!.document.links[id]!.muted },
      );
      if (commands.length) gestureCommand(view, { type: "batch", commands });
      else view.scheduler.request();
      return;
    }
  }
  if (event.phase === "down" && event.button === 2) {
    const canceled = cancelGesture(view),
      menuLayout = layoutStore?.view(currentTransform(view)),
      open = !!nodeMenuRequestId && !canceled && !!menuLayout && hitTest(menuLayout, event.position).kind === "canvas";
    if (nodeMenuRequestId)
      post(
        open
          ? {
              protocol: PROTOCOL_VERSION,
              type: "view.node-menu.result",
              viewId: view.id,
              requestId: nodeMenuRequestId,
              open: true,
              compositionRevision,
              viewPosition: event.position,
            }
          : {
              protocol: PROTOCOL_VERSION,
              type: "view.node-menu.result",
              viewId: view.id,
              requestId: nodeMenuRequestId,
              open: false,
            },
      );
    view.scheduler.request();
    return;
  }
  if (event.phase === "move") {
    const hover = hitTest(view.currentLayout, event.position);
    if (hover.kind === "control" || hover.kind === "control-step" || hover.kind === "ramp") {
      view.session.hoveredControl = hover.id;
      if (hover.kind === "ramp") view.session.hoveredRampTarget = hover.target;
      else delete view.session.hoveredRampTarget;
    } else {
      delete view.session.hoveredControl;
      delete view.session.hoveredRampTarget;
      if (!view.session.controlEdit) delete view.session.focusedControl;
    }
  }
  if (event.phase === "cancel") {
    cancelGesture(view);
    view.scheduler.request();
    return;
  }
  if (view.session.reroutePress?.pointerId === event.pointerId) {
    const press = view.session.reroutePress;
    if (event.phase === "move" && Math.hypot(event.position.x - press.start.x, event.position.y - press.start.y) >= 4) {
      delete view.session.reroutePress;
      view.session.linkDrag = { pointerId: event.pointerId, from: press.socketId, current: event.position };
      const hit = hitTest(view.gestureLayout!, event.position, "input");
      if (
        hit.kind === "socket" &&
        compatibleTargets(view.gestureLayout!, press.socketId).some((socket) => socket.id === hit.id)
      )
        view.session.linkDrag.candidate = hit.id;
      view.scheduler.request();
      return;
    }
    if (event.phase === "up") {
      delete view.session.reroutePress;
      view.gestureLayout = undefined;
      selectNode(view, press.nodeId, (event.modifiers & 8) !== 0);
      refreshLayout(view);
      view.scheduler.request();
      return;
    }
    return;
  }
  if (view.session.scrub?.pointerId === event.pointerId) {
    if (event.phase === "down" && event.button === 2) {
      cancelGesture(view);
      view.scheduler.request();
      return;
    }
    if (event.phase === "move") {
      const control = view.gestureLayout?.controls.get(view.session.scrub.controlId);
      if (Math.abs(event.position.x - view.session.scrub.startX) >= 2) view.session.scrub.moved = true;
      if (!view.session.scrub.moved) return;
      const raw = view.session.scrub.original as { kind?: unknown; value?: unknown };
      if (control && raw.kind === "json" && isColorRamp(raw.value)) {
        const stop = activeStop(view, control.id, raw.value),
          delta = (event.position.x - view.session.scrub.startX) * ((event.modifiers & 8) !== 0 ? 0.001 : 0.01);
        const ramp =
          view.session.scrub.component < 0
            ? moveRampStop(raw.value, stop.id, stop.position + delta)
            : setRampColor(
                raw.value,
                stop.id,
                stop.color.map((c, i) => (i === view.session.scrub!.component ? c + delta : c)) as unknown as readonly [
                  number,
                  number,
                  number,
                  number,
                ],
              );
        view.session.previewValues.set(control.id, {
          kind: "json",
          value: ramp as unknown as import("../core/types.js").JsonValue,
        });
        view.scheduler.request();
      } else if (control) {
        const value = scrubValue(
          control,
          view.session.scrub.original,
          view.session.scrub.component,
          event.position.x - view.session.scrub.startX,
          (event.modifiers & 8) !== 0,
          (event.modifiers & 2) !== 0,
        );
        view.session.previewValues.set(control.id, value);
        view.scheduler.request();
      }
    } else {
      const scrub = view.session.scrub,
        id = scrub.controlId,
        value = view.session.previewValues.get(id),
        moved = scrub.moved;
      cancelGesture(view);
      if (!moved) {
        view.session.focusedControl = id;
        if (scrub.original.kind !== "json") {
          const original = scrub.original,
            component =
              original.kind === "number"
                ? original.value
                : original.kind === "vector" || original.kind === "color"
                  ? (original.value[scrub.component] ?? 0)
                  : 0;
          view.session.controlEdit = {
            kind: "number",
            controlId: id,
            component: scrub.component,
            buffer: component.toFixed(3),
            selectAll: true,
          };
        }
        view.scheduler.request();
        return;
      }
      const command = value ? controlCommand(view, id, value) : undefined;
      if (command) gestureCommand(view, command);
      else view.scheduler.request();
    }
    return;
  }
  if (view.session.rampDrag?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      const found = rampValue(view, view.session.rampDrag.controlId, view.gestureLayout),
        b = found?.control.rampBounds;
      if (found && b) {
        const world = viewToWorld(event.position, view.gestureLayout!.transform),
          p = Math.max(0, Math.min(1, (world.x - b.handles.x) / b.handles.width));
        view.session.previewValues.set(found.control.id, {
          kind: "json",
          value: moveRampStop(
            found.ramp,
            view.session.rampDrag.stopId,
            p,
          ) as unknown as import("../core/types.js").JsonValue,
        });
        view.scheduler.request();
      }
    } else {
      const drag = view.session.rampDrag,
        value = view.session.previewValues.get(drag.controlId) as { kind?: unknown; value?: unknown } | undefined;
      cancelGesture(view);
      if (value?.kind === "json" && isColorRamp(value.value))
        commitRamp(view, drag.controlId, value.value, drag.stopId);
      else view.scheduler.request();
    }
    return;
  }
  if (event.phase === "down" && event.button === 1) {
    view.session.pan = { pointerId: event.pointerId, last: event.position };
    return;
  }
  if (view.session.pan?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      view.session.cameraCenter = {
        x: view.session.cameraCenter.x - (event.position.x - view.session.pan.last.x) / view.session.zoom,
        y: view.session.cameraCenter.y + (event.position.y - view.session.pan.last.y) / view.session.zoom,
      };
      view.session.pan = { ...view.session.pan, last: event.position };
      view.scheduler.request();
    } else delete view.session.pan;
    return;
  }
  if (view.session.modalMove) {
    if (event.phase === "move") {
      previewMove(view, view.session.modalMove, event.position);
      return;
    }
    if (event.phase === "down" && event.button === 0) {
      finishMove(view);
      return;
    }
  }
  if (event.phase === "down" && event.button === 0) {
    const previousEdit = view.session.controlEdit;
    if (previousEdit) {
      delete view.session.controlEdit;
      view.scheduler.request();
    }
    const interactionLayout = layoutStore
      ? applyNodeOrder(layoutStore.view(currentTransform(view)), view.session.uiOrder)
      : view.currentLayout;
    let hit = hitTest(interactionLayout, event.position, "output");
    if (
      previousEdit?.kind === "number" &&
      hit.kind === "control-step" &&
      hit.id === previousEdit.controlId &&
      hit.component === previousEdit.component
    )
      hit = { kind: "control", id: hit.id, component: hit.component };
    if (hit.kind === "resource") {
      const control = interactionLayout.controls.get(hit.id),
        policy = control?.resourceId && application?.compiled.resources.get(control.resourceId as never);
      if (resourceOpenRequestId && control?.kind === "resource" && control.source === "parameter" && policy)
        post({
          protocol: PROTOCOL_VERSION,
          type: "view.resource.open",
          viewId: view.id,
          requestId: resourceOpenRequestId,
          authorization: {
            viewId: view.id,
            token: control.id,
            graphVersion: state.version,
            compositionRevision,
          },
          resource: {
            id: policy.id,
            kind: "image",
            title: control.label,
            openTitle: control.openTitle ?? policy.openTitle ?? "Open",
            accept: [...policy.accept],
            maxBytes: policy.maxBytes,
            maxWidth: policy.maxWidth,
            maxHeight: policy.maxHeight,
            maxPixels: policy.maxPixels,
          },
        });
      return;
    }
    view.gestureLayout = interactionLayout;
    if (hit.kind === "ramp") {
      const found = rampValue(view, hit.id);
      if (!found) return;
      view.session.focusedControl = hit.id;
      view.session.focusedRampTarget = hit.target;
      const active = activeStop(view, hit.id, found.ramp);
      if (hit.target === "handle" && hit.stopId) {
        const same = found.ramp.stops.filter(
          (s) =>
            Math.abs(s.position - (hit.position ?? s.position)) <
            7 / view.currentLayout!.transform.zoom / found.control.rampBounds!.handles.width,
        );
        let id = hit.stopId;
        if (same.length > 1) {
          const old = view.session.activeRampStopByControl.get(found.control.id),
            i = same.findIndex((s) => s.id === old);
          id = same[(i + 1) % same.length]!.id;
        }
        view.session.activeRampStopByControl.set(found.control.id, id);
        view.session.rampDrag = {
          pointerId: event.pointerId,
          controlId: hit.id,
          stopId: id,
          original: found.control.value as import("../core/types.js").ParameterValue,
        };
        view.scheduler.request();
        return;
      }
      if (hit.target === "gradient") {
        rampAction(view, hit.id, "gradient", hit.position);
        return;
      }
      if (hit.target === "position") {
        view.session.scrub = {
          pointerId: event.pointerId,
          controlId: hit.id,
          component: -1,
          startX: event.position.x,
          original: found.control.value as import("../core/types.js").ParameterValue,
          moved: false,
        };
        return;
      }
      if (hit.target === "selector") {
        const i = found.ramp.stops.findIndex((s) => s.id === active.id);
        view.session.activeRampStopByControl.set(
          found.control.id,
          found.ramp.stops[(i + 1) % found.ramp.stops.length]!.id,
        );
        view.scheduler.request();
        return;
      }
      if (hit.target === "swatch") {
        openRampPicker(view, hit.id, found.ramp);
        return;
      }
      rampAction(view, hit.id, hit.target);
      view.scheduler.request();
      return;
    }
    if (hit.kind === "color-wheel") {
      const control = view.currentLayout.controls.get(hit.id),
        value = control?.value as import("../core/types.js").ParameterValue | undefined,
        bounds = control?.colorWheelBounds?.[hit.region];
      if (!control || value?.kind !== "color" || !bounds) return;
      const rgba = value.value as Rgba;
      view.gestureLayout = view.currentLayout;
      view.session.colorWheel = {
        controlId: hit.id,
        original: value,
        model: oklabToOklch(srgbToOklab([rgba[0], rgba[1], rgba[2]])),
        rgba,
        pointerId: event.pointerId,
        region: hit.region,
        bounds: viewRectFromWorld(bounds, view.currentLayout),
      };
      updateInlineWheel(view, event.position);
      return;
    }
    if (hit.kind === "control-step") {
      const control = view.currentLayout.controls.get(hit.id),
        original = control?.value as import("../core/types.js").ParameterValue | undefined;
      if (control && original) {
        view.session.focusedControl = hit.id;
        const current =
            original.kind === "number"
              ? original.value
              : original.kind === "vector" || original.kind === "color"
                ? (original.value[hit.component] ?? 0)
                : 0,
          next = setNumericComponent(
            control,
            original,
            hit.component,
            current + hit.direction * numericStep(control, (event.modifiers & 8) !== 0),
          ),
          command = controlCommand(view, hit.id, next);
        if (command) gestureCommand(view, command);
      }
      return;
    }
    if (hit.kind === "control") {
      const control = view.currentLayout.controls.get(hit.id);
      view.session.focusedControl = hit.id;
      if (control && !control.linked && control.source !== "unknown") {
        const current = control.value as import("../core/types.js").ParameterValue;
        if (control.kind === "boolean" && current.kind === "boolean") {
          const command = controlCommand(view, hit.id, { kind: "boolean", value: !current.value });
          if (command) gestureCommand(view, command);
        } else if (
          control.kind === "enum" &&
          control.schema?.type === "string" &&
          control.schema.enum &&
          current.kind === "string"
        ) {
          const command = controlCommand(view, hit.id, {
            kind: "string",
            value: cycleEnum(control.schema.enum, current.value, 1),
          });
          if (command) gestureCommand(view, command);
        } else if ((control.kind === "string" || control.kind === "resource") && current.kind === "string")
          view.session.controlEdit = { kind: "string", controlId: hit.id, buffer: current.value };
        else if (control.kind === "color" && current.kind === "color") openControlPicker(view, hit.id);
        else if (control.kind === "number" || control.kind === "vector")
          view.session.scrub = {
            pointerId: event.pointerId,
            controlId: hit.id,
            component: hit.component,
            startX: event.position.x,
            original: current,
            moved: false,
          };
      }
      view.scheduler.request();
      return;
    }
    if (hit.kind === "socket") {
      const socket = view.currentLayout.sockets.get(hit.id),
        node = socket && view.currentLayout.nodes.get(socket.nodeId);
      if (socket?.direction === "output" && node?.kind === "reroute")
        view.session.reroutePress = {
          pointerId: event.pointerId,
          nodeId: node.id,
          socketId: hit.id,
          start: event.position,
        };
      else if (socket?.direction === "output")
        view.session.linkDrag = { pointerId: event.pointerId, from: hit.id, current: event.position };
      view.scheduler.request();
      return;
    }
    if (hit.kind === "collapse") {
      const n = state.document.nodes[hit.id];
      if (n) gestureCommand(view, { type: "node.collapse", id: hit.id, value: !n.collapsed });
      return;
    }
    if (hit.kind === "resize") {
      view.session.resize = { pointerId: event.pointerId, id: hit.id };
      return;
    }
    if (hit.kind === "link") {
      view.session.selectedNodes.clear();
      if ((event.modifiers & 8) === 0) view.session.selectedLinks.clear();
      view.session.selectedLinks.add(hit.id);
      view.scheduler.request();
      return;
    }
    if (hit.kind === "canvas") {
      view.session.box = {
        pointerId: event.pointerId,
        start: event.position,
        current: event.position,
        checkpoint: new Set(view.session.selectedNodes),
        add: (event.modifiers & 8) !== 0,
      };
      if (!view.session.box.add) view.session.selectedNodes.clear();
      view.scheduler.request();
      return;
    }
    const id = hit.id;
    if ((event.modifiers & 8) !== 0) selectNode(view, id, true);
    else if (!view.session.selectedNodes.has(id)) selectNode(view, id);
    else {
      view.session.activeNode = id;
      raiseNode(view, id);
    }
    refreshLayout(view);
    view.gestureLayout = view.currentLayout;
    const roots = groupRoots(view.session.selectedNodes, view.currentLayout);
    view.session.drag = {
      pointerId: event.pointerId,
      startView: event.position,
      startWorld: viewToWorld(event.position, view.currentLayout.transform),
      origins: new Map(roots.map((root) => [root, state!.document.nodes[root]!.position])),
      moved: false,
    };
    view.scheduler.request();
    return;
  }
  if (view.session.box?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      view.session.box.current = event.position;
      const found = boxNodes(view.gestureLayout!, view.session.box.start, event.position);
      view.session.selectedNodes = new Set(view.session.box.add ? [...view.session.box.checkpoint, ...found] : found);
      view.scheduler.request();
    } else {
      for (const id of view.gestureLayout!.drawOrder) if (view.session.selectedNodes.has(id)) raiseNode(view, id);
      delete view.session.box;
      view.gestureLayout = undefined;
      refreshLayout(view);
      view.scheduler.request();
    }
    return;
  }
  if (view.session.linkDrag?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      view.session.linkDrag.current = event.position;
      const hit = hitTest(view.gestureLayout!, event.position, "input");
      if (
        hit.kind === "socket" &&
        compatibleTargets(view.gestureLayout!, view.session.linkDrag.from).some((s) => s.id === hit.id)
      )
        view.session.linkDrag.candidate = hit.id;
      else delete view.session.linkDrag.candidate;
      view.scheduler.request();
    } else {
      const command = view.session.linkDrag.candidate
        ? planLink(
            view.gestureLayout!,
            view.session.linkDrag.from,
            view.session.linkDrag.candidate,
            linkId(`gesture-link-${state.version + 1}`),
          )
        : undefined;
      cancelGesture(view);
      if (command) gestureCommand(view, command);
      else view.scheduler.request();
    }
    return;
  }
  if (view.session.resize?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      const size = clampResize(
        view.gestureLayout!,
        view.session.resize.id,
        viewToWorld(event.position, view.gestureLayout!.transform),
      );
      if (size) view.session.previewSizes.set(view.session.resize.id, size);
      view.scheduler.request();
    } else {
      const id = view.session.resize.id,
        size = view.session.previewSizes.get(id);
      cancelGesture(view);
      if (size) gestureCommand(view, { type: "node.resize", id, size });
      else view.scheduler.request();
    }
    return;
  }
  if (view.session.drag?.pointerId === event.pointerId) {
    if (event.phase === "move") {
      const distance = Math.hypot(
        event.position.x - view.session.drag.startView.x,
        event.position.y - view.session.drag.startView.y,
      );
      if (distance >= 4) view.session.drag.moved = true;
      if (view.session.drag.moved) previewMove(view, view.session.drag, event.position);
    } else if (event.phase === "up") finishMove(view);
    return;
  }
}
function input(
  view: WorkerView,
  event: Extract<import("../browser/protocol.js").WorkerRequest, { type: "view.input" }>["event"],
  nodeMenuRequestId?: string,
  resourceOpenRequestId?: string,
): void {
  try {
    processInput(view, event, nodeMenuRequestId, resourceOpenRequestId);
  } finally {
    publishSelection(view);
  }
}

function previewMove(
  view: WorkerView,
  move: {
    startWorld: { x: number; y: number };
    origins: ReadonlyMap<import("../core/types.js").NodeId, { x: number; y: number }>;
  },
  position: { x: number; y: number },
): void {
  const layout = view.gestureLayout ?? view.currentLayout!;
  const now = viewToWorld(position, layout.transform);
  for (const [id, p] of move.origins)
    view.session.previewPositions.set(id, { x: p.x + now.x - move.startWorld.x, y: p.y + now.y - move.startWorld.y });
  const first = [...move.origins][0];
  if (first) {
    const n = layout.nodes.get(first[0]);
    const preview = view.session.previewPositions.get(first[0]);
    const target =
      n && preview
        ? frameDropCandidate(layout, first[0], {
            x: n.worldPosition.x + preview.x - n.localPosition.x + n.bounds.width / 2,
            y: n.worldPosition.y + preview.y - n.localPosition.y - n.bounds.height / 2,
          })
        : undefined;
    if (target) view.session.parentHighlight = target;
    else delete view.session.parentHighlight;
  }
  view.scheduler.request();
}
function finishMove(view: WorkerView): void {
  const layout = view.gestureLayout ?? view.currentLayout!;
  const commands: import("../commands/types.js").BatchCommand[] = [...view.session.previewPositions].map(
    ([id, position]) => ({ type: "node.move", id, position }),
  );
  for (const [id] of view.session.previewPositions) {
    const n = layout.nodes.get(id);
    if (!n) continue;
    const target = view.session.parentHighlight;
    if ((target ?? undefined) !== (n.parentId ?? undefined))
      commands.push({ type: "node.parent", id, parentId: target ?? null });
  }
  cancelGesture(view);
  if (commands.length) gestureCommand(view, { type: "batch", commands });
  else view.scheduler.request();
}

function resourceTarget(view: WorkerView, token: string, fresh = false) {
  const control = (fresh && layoutStore ? layoutStore.view(currentTransform(view)) : view.currentLayout)?.controls.get(
    token,
  );
  return control?.kind === "resource" && control.source === "parameter" ? control : undefined;
}
function closeResourceImages(): void {
  for (const image of resourceImages.values()) image.bitmap.close();
  resourceImages.clear();
  resourceOperations.clear();
}
function trimResourceImages(): void {
  let bytes = [...resourceImages.values()].reduce((sum, image) => sum + image.bytes, 0);
  while (resourceImages.size > 16 || bytes > 128 * 1024 * 1024) {
    const oldest = [...resourceImages].sort((a, b) => a[1].lastUsed - b[1].lastUsed)[0];
    if (!oldest) break;
    oldest[1].bitmap.close();
    bytes -= oldest[1].bytes;
    resourceImages.delete(oldest[0]);
  }
}
async function setResource(
  view: WorkerView,
  data: Extract<import("../browser/protocol.js").WorkerRequest, { type: "view.resource.set" }>,
): Promise<void> {
  applyPointerFence(view, data.pointerFence);
  const { authorization, resource } = data;
  if (authorization.viewId !== view.id) {
    reject(data.id, { code: "resource.view-mismatch", message: "Resource authorization belongs to another view" });
    return;
  }
  if (
    !state ||
    authorization.graphVersion !== state.version ||
    authorization.compositionRevision !== compositionRevision
  ) {
    reject(data.id, { code: "resource.stale", message: "The image authorization is stale" });
    return;
  }
  if (data.expected.kind === "exact" && data.expected.version !== state.version) {
    reject(data.id, { code: "version.stale", message: "Expected version does not match" });
    return;
  }
  const decodeRevision = compositionRevision,
    decodeGraphVersion = state.version,
    target = resourceTarget(view, authorization.token, true),
    targetNode = target && state.document.nodes[target.nodeId];
  if (!target || !targetNode) {
    reject(data.id, { code: "resource.stale", message: "The image target is no longer available" });
    return;
  }
  const policy = target.resourceId && application?.compiled.resources.get(target.resourceId as never);
  if (!policy) {
    reject(data.id, { code: "resource.policy", message: "The image resource policy is unavailable" });
    return;
  }
  if (resource.bytes.byteLength > Math.min(policy.maxBytes, 32 * 1024 * 1024)) {
    reject(data.id, { code: "resource.bytes", message: "The selected file is too large" });
    return;
  }
  const operation = { serial: ++resourceSerial, view };
  resourceOperations.set(authorization.token, operation);
  try {
    await setResourceRegistered(
      view,
      data,
      operation,
      target,
      targetNode,
      policy,
      state,
      decodeRevision,
      decodeGraphVersion,
    );
  } finally {
    if (resourceOperations.get(authorization.token) === operation) resourceOperations.delete(authorization.token);
  }
}
async function setResourceRegistered(
  view: WorkerView,
  data: Extract<import("../browser/protocol.js").WorkerRequest, { type: "view.resource.set" }>,
  operation: { serial: number; view: WorkerView },
  target: NonNullable<ReturnType<typeof resourceTarget>>,
  targetNode: BoundEngineState["document"]["nodes"][string],
  policy: {
    readonly maxWidth: number;
    readonly maxHeight: number;
    readonly maxPixels: number;
    readonly referencePrefix: string;
  },
  decodeState: BoundEngineState,
  decodeRevision: number,
  decodeGraphVersion: number,
): Promise<void> {
  const { authorization, resource } = data,
    sourceView = view;
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(new Blob([resource.bytes], { type: resource.mime || "application/octet-stream" }));
  } catch {
    reject(data.id, { code: "resource.decode", message: "The selected file is not a supported image" });
    return;
  }
  const current = resourceTarget(view, authorization.token, true);
  if (
    views.get(sourceView.id) !== sourceView ||
    state !== decodeState ||
    decodeState.version !== decodeGraphVersion ||
    decodeState.document.nodes[target.nodeId] !== targetNode ||
    compositionRevision !== decodeRevision ||
    resourceOperations.get(authorization.token) !== operation ||
    !current ||
    current.nodeId !== target.nodeId ||
    current.key !== target.key ||
    current.resourceId !== target.resourceId
  ) {
    bitmap.close();
    reject(data.id, { code: "resource.stale", message: "The image target changed while decoding" });
    return;
  }
  if (
    bitmap.width > Math.min(policy.maxWidth, 8192) ||
    bitmap.height > Math.min(policy.maxHeight, 8192) ||
    bitmap.width * bitmap.height > Math.min(policy.maxPixels, 16_777_216)
  ) {
    bitmap.close();
    reject(data.id, { code: "resource.dimensions", message: "The decoded image is too large" });
    return;
  }
  const cancelled = cancelGesture(view);
  publishSelection(view);
  if (cancelled) view.scheduler.request();
  const finalTarget = resourceTarget(view, authorization.token, true);
  if (
    !finalTarget ||
    finalTarget.nodeId !== target.nodeId ||
    finalTarget.key !== target.key ||
    finalTarget.resourceId !== target.resourceId
  ) {
    bitmap.close();
    reject(data.id, { code: "resource.stale", message: "The image target changed before commit" });
    return;
  }
  const reference = `${policy.referencePrefix}${decodeState.document.graphId}:${++resourceSerial}:${encodeURIComponent(resource.name)}`,
    result = application!.runtime.transition(decodeState, {
      commandId: commandId(data.id),
      expectedVersion: decodeState.version,
      source: "gesture",
      command: {
        type: "node.parameter",
        id: target.nodeId,
        key: target.key,
        value: { kind: "string", value: reference },
      },
    });
  if (result.status === "rejected") {
    bitmap.close();
    reject(data.id, result.error);
    return;
  }
  if (result.status === "noop") {
    bitmap.close();
    post({
      protocol: PROTOCOL_VERSION,
      type: "response",
      id: data.id,
      ok: true,
      value: { status: "noop", version: decodeState.version },
    });
    return;
  }
  commit(result, {
    type: "node.parameter",
    id: target.nodeId,
    key: target.key,
    value: { kind: "string", value: reference },
  });
  resourceImages.set(reference, {
    bitmap,
    name: resource.name,
    bytes: bitmap.width * bitmap.height * 4,
    lastUsed: performance.now(),
  });
  trimResourceImages();
  post({
    protocol: PROTOCOL_VERSION,
    type: "response",
    id: data.id,
    ok: true,
    value: { status: "committed", version: state.version },
  });
}

function adoptHostGeneration(view: WorkerView, next: number): boolean {
  if (next < view.hostGeneration) return false;
  const advanced = next > view.hostGeneration;
  view.hostGeneration = next;
  if (advanced) view.scheduler.request(undefined, DirtyReason.HostInteraction);
  return true;
}
function applyPointerSnapshot(
  view: WorkerView,
  snapshot: import("../browser/pointer-lane.js").PointerLaneSnapshot | undefined,
): void {
  if (snapshot && snapshot.sequence !== view.consumedPointerSequence) {
    view.consumedPointerSequence = snapshot.sequence;
    if (!adoptHostGeneration(view, snapshot.hostGeneration)) return;
    input(view, snapshot.event);
  }
}
function applyPointerFence(view: WorkerView, fence: PointerFence | undefined): void {
  if (!fence) return;
  applyPointerSnapshot(view, fence.before);
  view.handledPointerFence = fence.generation;
}
function pollPointerLane(view: WorkerView): void {
  if (!view.pointerLane) return;
  const generation = pointerLaneFence(view.pointerLane);
  if (generation !== view.handledPointerFence) return;
  const move = readPointerMove(view.pointerLane, view.consumedPointerSequence);
  if (pointerLaneFence(view.pointerLane) !== generation) return;
  applyPointerSnapshot(view, move);
}

type ApplicationAuthority = NonNullable<typeof application>;
interface StagedCompositionUpdate {
  readonly baseRevision: number;
  readonly revision: number;
  readonly change: CompositionChange;
  readonly baseGraphVersion: number;
  readonly graphVersion: number;
  readonly graphChanged: boolean;
  readonly application: ApplicationAuthority;
  readonly state: BoundEngineState;
  readonly layoutStore: IndexedLayoutStore<FxNodeCompositionData>;
  readonly mutationEnvelope?: BoundMutationEnvelope;
  readonly snapshotEnvelope?: BoundSnapshotEnvelope;
}
function putCompositionEntry(
  record: Readonly<Record<string, unknown>>,
  id: string,
  value: unknown,
): Readonly<Record<string, unknown>> {
  const entries: Array<readonly [string, unknown]> = [];
  let replaced = false;
  for (const [key, current] of Object.entries(record)) {
    if (key === id) {
      entries.push([key, value]);
      replaced = true;
    } else entries.push([key, current]);
  }
  if (!replaced) entries.push([id, value]);
  return nullRecord(entries);
}
function removeCompositionEntry(
  record: Readonly<Record<string, unknown>>,
  id: string,
): Readonly<Record<string, unknown>> {
  return nullRecord(Object.entries(record).filter(([key]) => key !== id));
}
function patchCompositionSource(source: FxNodeCompositionData, update: CompositionUpdateWire): unknown {
  switch (update.kind) {
    case "composition.load":
      return update.composition;
    case "theme.set":
      return { ...source, theme: update.theme };
    case "header-styles.set":
      return { ...source, nodeStyles: update.styles };
    case "compatibility.set":
      return { ...source, compatibility: update.compatibility };
    case "socket.compose":
      return { ...source, socketTypes: putCompositionEntry(source.socketTypes, update.id, update.definition) };
    case "socket.remove":
      return { ...source, socketTypes: removeCompositionEntry(source.socketTypes, update.id) };
    case "node.compose":
      return { ...source, nodes: putCompositionEntry(source.nodes, update.id, update.definition) };
    case "node.remove":
      return { ...source, nodes: removeCompositionEntry(source.nodes, update.id) };
  }
}
const compositionChange = (update: CompositionUpdateWire): CompositionChange =>
  update.kind === "theme.set" ||
  update.kind === "header-styles.set" ||
  update.kind === "composition.load" ||
  update.kind === "compatibility.set"
    ? { kind: update.kind }
    : { kind: update.kind, id: update.id };
type StageResult =
  | { readonly status: "noop"; readonly receipt: CompositionReceipt }
  | { readonly status: "rejected"; readonly error: WireError }
  | { readonly status: "committed"; readonly staged: StagedCompositionUpdate };
function stageCompositionUpdate(data: Extract<WorkerRequest, { type: "composition.update" }>): StageResult {
  const currentApplication = application!,
    currentState = state!;
  if (data.expected.kind === "exact" && data.expected.revision !== compositionRevision)
    return {
      status: "rejected",
      error: toWireError({
        code: "composition.revision.stale",
        message: "Expected composition revision does not match",
      }),
    };
  let candidateCompiled: CompiledFxNodeComposition<FxNodeCompositionData>;
  try {
    candidateCompiled = compileFxNodeComposition(
      patchCompositionSource(currentApplication.compiled.source, data.update) as FxNodeCompositionData,
    );
  } catch (error) {
    if (error instanceof FxNodeCompositionError)
      return {
        status: "rejected",
        error: toWireError({ code: "composition.invalid", message: error.message, issues: error.issues }),
      };
    throw error;
  }
  if (canonicalJsonEqual(candidateCompiled.source, currentApplication.compiled.source))
    return {
      status: "noop",
      receipt: {
        status: "noop",
        revision: compositionRevision,
        graphVersion: currentState.version,
        graphChanged: false,
        historyReset: false,
      },
    };
  if (compositionRevision >= Number.MAX_SAFE_INTEGER)
    return {
      status: "rejected",
      error: toWireError({ code: "composition.revision.overflow", message: "Composition revision exhausted" }),
    };
  const removedNodeTypes =
    data.update.kind === "composition.load"
      ? new Set(
          Object.keys(currentApplication.compiled.source.nodes).filter(
            (id) => !Object.hasOwn(candidateCompiled.source.nodes, id),
          ),
        )
      : data.update.kind === "node.remove"
        ? new Set([data.update.id])
        : undefined;
  const candidateRuntime = bindFxNodeHeadless(candidateCompiled),
    rebound = rebindBoundEngineAuthority(currentState, currentApplication.runtime, candidateRuntime, {
      commandId: commandId(data.id),
      ...(removedNodeTypes ? { removedNodeTypes } : {}),
    });
  if (!rebound.ok) {
    const demotion = rebound.issues.some((issue) => issue.code === "composition.node-demotion"),
      overflow = rebound.issues.some((issue) => issue.code === "version.overflow");
    return {
      status: "rejected",
      error: toWireError({
        code: demotion ? "composition.node-demotion" : overflow ? "version.overflow" : "composition.rebind.invalid",
        message: demotion
          ? "Composition update would demote a known node"
          : overflow
            ? "Graph version exhausted"
            : "Current graph is incompatible with the composition update",
        issues: rebound.issues,
      }),
    };
  }
  const candidateStore = new IndexedLayoutStore(candidateCompiled, rebound.state.document);
  const revision = compositionRevision + 1;
  return {
    status: "committed",
    staged: {
      baseRevision: compositionRevision,
      revision,
      change: compositionChange(data.update),
      baseGraphVersion: currentState.version,
      graphVersion: rebound.state.version,
      graphChanged: rebound.graphChanged,
      application: { compiled: candidateCompiled, runtime: candidateRuntime },
      state: rebound.state,
      layoutStore: candidateStore,
      ...(rebound.graphChanged
        ? { mutationEnvelope: rebound.mutationEnvelope, snapshotEnvelope: rebound.snapshotEnvelope }
        : {}),
    },
  };
}
function publishCompositionUpdate(id: string, staged: StagedCompositionUpdate): void {
  for (const view of views.values()) cancelGesture(view);
  application = staged.application;
  state = staged.state;
  layoutStore = staged.layoutStore;
  compositionRevision = staged.revision;
  const nodeIds = new Set(Object.keys(state.document.nodes) as import("../core/types.js").NodeId[]),
    linkIds = new Set(Object.keys(state.document.links) as import("../core/types.js").LinkId[]);
  for (const view of views.values()) {
    const order = view.session.uiOrder.filter((nodeId) => nodeIds.has(nodeId));
    resetSessionForCompositionRebind(view.session, nodeIds, linkIds, order);
    view.gestureLayout = undefined;
    view.currentLayout = applyNodeOrder(layoutStore.view(currentTransform(view)), order);
  }
  journal = checkpointJournal(application.runtime.save(state.document));
  const envelope: CompositionChangeEnvelope = {
    baseRevision: staged.baseRevision,
    revision: staged.revision,
    change: staged.change,
    baseGraphVersion: staged.baseGraphVersion,
    graphVersion: staged.graphVersion,
    graphChanged: staged.graphChanged,
    historyReset: true,
  };
  post({ protocol: PROTOCOL_VERSION, type: "composition.event", envelope });
  if (staged.graphChanged) {
    post({ protocol: PROTOCOL_VERSION, type: "mutation", envelope: staged.mutationEnvelope! });
    post({ protocol: PROTOCOL_VERSION, type: "snapshot.event", envelope: staged.snapshotEnvelope! });
  }
  for (const view of views.values()) publishSelection(view);
  for (const view of views.values()) view.scheduler.request();
  post({
    protocol: PROTOCOL_VERSION,
    type: "response",
    id,
    ok: true,
    value: {
      status: "committed",
      revision: staged.revision,
      graphVersion: staged.graphVersion,
      graphChanged: staged.graphChanged,
      historyReset: true,
    },
  });
}
function recognizableInvalidRpc(value: unknown):
  | {
      readonly type:
        | "init"
        | "command"
        | "load"
        | "save.data"
        | "state.set"
        | "composition.update"
        | "view.attach"
        | "view.detach"
        | "view.viewport"
        | "view.node.add"
        | "view.selection.remove"
        | "view.selection.mute"
        | "view.resource.set";
      readonly id: string;
    }
  | undefined {
  try {
    if (typeof value !== "object" || value === null || (value as { protocol?: unknown }).protocol !== PROTOCOL_VERSION)
      return;
    const type = (value as { type?: unknown }).type,
      id = (value as { id?: unknown }).id;
    if (
      (type !== "init" &&
        type !== "command" &&
        type !== "load" &&
        type !== "save.data" &&
        type !== "state.set" &&
        type !== "composition.update" &&
        type !== "view.attach" &&
        type !== "view.detach" &&
        type !== "view.viewport" &&
        type !== "view.node.add" &&
        type !== "view.selection.remove" &&
        type !== "view.selection.mute" &&
        type !== "view.resource.set") ||
      typeof id !== "string" ||
      !id.length ||
      id.length > 512
    )
      return;
    return { type, id };
  } catch {
    return;
  }
}

async function handleMessage({ data }: MessageEvent<unknown>): Promise<void> {
  if (!validRequest(data)) {
    const recognizable = recognizableInvalidRpc(data);
    if (recognizable) {
      const candidate = data as { viewId?: unknown };
      if (
        recognizable.type.startsWith("view.") &&
        recognizable.type !== "view.attach" &&
        (typeof candidate.viewId !== "string" || !views.has(candidate.viewId))
      ) {
        reject(recognizable.id, { code: "view.missing", message: "View is not attached" });
        return;
      }
      reject(recognizable.id, {
        code:
          recognizable.type === "init"
            ? "init.invalid"
            : recognizable.type === "command"
              ? "command.invalid"
              : recognizable.type === "load"
                ? "layout.request.invalid"
                : recognizable.type === "save.data"
                  ? "save-data.request.invalid"
                  : recognizable.type === "state.set"
                    ? "state.request.invalid"
                    : recognizable.type === "composition.update"
                      ? "composition.request.invalid"
                      : recognizable.type === "view.viewport"
                        ? "viewport.request.invalid"
                        : recognizable.type === "view.resource.set"
                          ? "resource.request.invalid"
                          : "action.request.invalid",
        message: `Invalid ${recognizable.type} request`,
      });
      return;
    }
    fatal(new Error("Invalid worker protocol message"), "protocol.invalid");
    return;
  }
  try {
    if (data.type === "init") {
      if (application) {
        reject(data.id, { code: "init.duplicate", message: "Worker is already initialized" });
        return;
      }
      const compiled = createInitialFxNodeComposition(
          data.applicationId as string,
          data.applicationVersion as number,
          data.resources as never,
        ),
        runtime = bindFxNodeHeadless(compiled);
      const nextState = runtime.createEngine(runtime.emptyDocument(), data.historyLimit),
        nextStore = new IndexedLayoutStore(compiled, nextState.document);
      application = { compiled, runtime };
      state = nextState;
      journal = checkpointJournal(runtime.save(nextState.document));
      layoutStore = nextStore;
      compositionRevision = 0;
      post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: true });
      return;
    }
    if (data.type === "view.attach") {
      if (!state) {
        reject(data.id, { code: "init.required", message: "Worker is not initialized" });
        return;
      }
      if (usedViewIds.has(data.viewId))
        return reject(data.id, { code: "view.id.used", message: "View ID has already been used" });
      usedViewIds.add(data.viewId);
      let allocated = false;
      try {
        const device = fxNodeDevicePixels(data.viewport.width, data.viewport.height, data.viewport.dpr),
          aggregate = [...views.values()].reduce((sum, item) => sum + item.deviceWidth * item.deviceHeight, 0);
        if (
          !device ||
          views.size >= FXNODE_VIEW_LIMITS.maxViews ||
          aggregate + device.width * device.height > FXNODE_VIEW_LIMITS.maxActiveDevicePixels
        )
          return reject(data.id, { code: "view.limit", message: "View rendering limits exceeded" });
        if (
          typeof OffscreenCanvas !== "function" ||
          typeof ImageBitmap !== "function" ||
          typeof ImageBitmap.prototype.close !== "function" ||
          (data.pointerLane !== undefined && data.pointerLane.byteLength !== Int32Array.BYTES_PER_ELEMENT * 16)
        )
          return reject(data.id, {
            code: "view.render.unavailable",
            message: "Required view rendering capabilities are unavailable",
          });
        const mutation = await atlas.attach(data.viewId, device);
        allocated = true;
        requestViewInvalidations(mutation.invalidatedViewIds, views);
        const session = createSession(data.camera);
        let view!: WorkerView;
        const scheduler = new RenderScheduler((frameId, renderId, reasons) => {
          void drawView(view, frameId, renderId, reasons).catch(fatal);
        });
        view = {
          id: data.viewId,
          session,
          viewport: data.viewport,
          deviceWidth: device.width,
          deviceHeight: device.height,
          surfaceGeneration: 0,
          resizePending: false,
          currentLayout: undefined,
          gestureLayout: undefined,
          scheduler,
          pointerLane: data.pointerLane,
          handledPointerFence: 0,
          consumedPointerSequence: 0,
          hostGeneration: 0,
          selectionSignature: "",
          inFlightPixels: undefined,
        };
        view.currentLayout = layoutStore!.view(currentTransform(view));
        views.set(view.id, view);
        post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: true });
        publishSelection(view, true);
        scheduler.request(1);
        scheduler.start(() => pollPointerLane(view));
      } catch (error) {
        if (error instanceof ViewAtlasError) {
          if (error.fatal) {
            fatal(error, error.code);
            return;
          }
          for (const item of views.values()) item.scheduler.request(0, DirtyReason.Viewport);
          reject(data.id, { code: error.code, message: error.message });
          return;
        }
        if (allocated)
          try {
            const cleanup = await atlas.detach(data.viewId);
            requestViewInvalidations(cleanup.invalidatedViewIds, views);
          } catch (cleanupError) {
            fatal(cleanupError);
            return;
          }
        reject(data.id, {
          code: "view.attach.rejected",
          message: error instanceof Error ? error.message : "Unable to attach view",
        });
      }
      return;
    }
    if (!state) {
      fatal(new Error("Worker is not initialized"));
      return;
    }
    if (data.type === "view.detach") {
      const view = views.get(data.viewId);
      if (!view) return reject(data.id, { code: "view.missing", message: "View is not attached" });
      cancelGesture(view);
      view.scheduler.stop();
      views.delete(data.viewId);
      for (const [token, operation] of resourceOperations)
        if (operation.view === view && resourceOperations.get(token) === operation) resourceOperations.delete(token);
      let detach: Awaited<ReturnType<ViewAtlasManager["detach"]>>;
      try {
        detach = await atlas.detach(data.viewId);
      } finally {
        releaseFramePixels(view);
      }
      for (const id of detach.invalidatedViewIds) views.get(id)?.scheduler.request(0, DirtyReason.Viewport);
      post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: true });
      return;
    }
    const view = "viewId" in data ? views.get(data.viewId) : undefined;
    if ("viewId" in data && !view) {
      if ("id" in data) reject(data.id, { code: "view.missing", message: "View is not attached" });
      return;
    }
    if (data.type === "composition.update") {
      const result = stageCompositionUpdate(data);
      if (result.status === "rejected")
        post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: false, error: result.error });
      else if (result.status === "noop")
        post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: true, value: result.receipt });
      else publishCompositionUpdate(data.id, result.staged);
      return;
    }
    if (data.type === "command") {
      for (const item of views.values()) {
        if (cancelGesture(item)) item.scheduler.request();
        publishSelection(item);
      }
      const result = application!.runtime.transition(state, {
        commandId: commandId(data.id),
        expectedVersion: data.expected.kind === "current" ? state.version : data.expected.version,
        source: "api",
        command: data.command,
      });
      if (result.status === "rejected") reject(data.id, result.error);
      else {
        if (result.status === "committed") commit(result, data.command);
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: result.status, version: state.version },
        });
      }
      return;
    }
    if (data.type === "state.set") {
      const result = application!.runtime.replaceState(state, {
        commandId: commandId(data.id),
        expectedVersion: data.expected.kind === "current" ? state.version : data.expected.version,
        target: data.state as never,
      });
      if (result.status === "rejected") reject(data.id, result.error);
      else {
        if (result.status === "committed")
          installDocumentReplacement(result, checkpointJournal(application!.runtime.save(result.state.document)));
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: result.status, version: state.version },
        });
      }
      return;
    }
    if (data.type === "view.node.add") {
      applyPointerFence(view!, data.pointerFence);
      const cancelled = cancelGesture(view!);
      publishSelection(view!);
      if (cancelled) view!.scheduler.request();
      const command: Command = {
          type: "node.add",
          nodeId: nodeId(data.nodeId),
          nodeType: data.typeId,
          position: viewToWorld(data.viewPosition, currentTransform(view!)),
        },
        result = application!.runtime.transition(state, {
          commandId: commandId(data.id),
          expectedVersion: data.expected.kind === "current" ? state.version : data.expected.version,
          source: "api",
          command,
        });
      if (result.status === "rejected") reject(data.id, result.error);
      else {
        if (result.status === "committed")
          commit(result, command, view!, { kind: "select-added", nodeId: data.nodeId });
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: result.status, version: state.version },
        });
      }
      return;
    }
    if (data.type === "view.selection.remove" || data.type === "view.selection.mute") {
      applyPointerFence(view!, data.pointerFence);
      const cancelled = cancelGesture(view!);
      publishSelection(view!);
      if (cancelled) view!.scheduler.request();
      const command =
        data.type === "view.selection.remove"
          ? planSelectionRemoval(state.document, view!.session.selectedNodes, view!.session.selectedLinks)
          : planSelectionMute(state.document, view!.session.selectedNodes, isStandardNode, data.value);
      if (!command) {
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: "noop", version: state.version },
        });
        return;
      }
      const result = application!.runtime.transition(state, {
        commandId: commandId(data.id),
        expectedVersion: data.expected.kind === "current" ? state.version : data.expected.version,
        source: "api",
        command,
      });
      if (result.status === "rejected") reject(data.id, result.error);
      else {
        if (result.status === "committed") commit(result, command);
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: result.status, version: state.version },
        });
      }
      return;
    }
    if (data.type === "view.resource.set") {
      void setResource(view!, data).catch((error) =>
        reject(data.id, {
          code: "resource.error",
          message: error instanceof Error ? error.message : "Unable to load image",
        }),
      );
      return;
    }
    if (data.type === "load") {
      const expected = data.expected.kind === "current" ? state.version : data.expected.version,
        isSaveData =
          typeof data.data === "object" &&
          data.data !== null &&
          (data.data as { kind?: unknown }).kind === "fxnode.command-log",
        result = isSaveData
          ? application!.runtime.replaySaveData(state, data.data, expected, commandId(data.id))
          : application!.runtime.load(state, data.data, expected, commandId(data.id));
      if (!result.ok)
        reject(data.id, {
          code: result.issues[0]?.code ?? "layout.invalid",
          message: result.issues[0]?.message ?? "Invalid layout",
          path: result.issues[0]?.path,
          issues: result.issues,
        });
      else if (isSaveData && "status" in result && result.status === "noop") {
        let cancelled = false;
        for (const item of views.values()) cancelled = cancelGesture(item) || cancelled;
        state = result.state;
        journal = importJournal(result.saveData);
        for (const item of views.values()) {
          publishSelection(item);
          if (cancelled) item.scheduler.request();
        }
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: "noop", version: state.version },
        });
      } else {
        const committed = result as
          | Extract<BoundLoadResult, { ok: true }>
          | Extract<BoundReplayResult, { ok: true; status: "committed" }>;
        installDocumentReplacement(
          committed,
          "saveData" in committed
            ? importJournal(committed.saveData)
            : checkpointJournal(application!.runtime.save(committed.state.document)),
        );
        post({
          protocol: PROTOCOL_VERSION,
          type: "response",
          id: data.id,
          ok: true,
          value: { status: "committed", version: state.version },
        });
      }
      return;
    }
    if (data.type === "state.get") {
      post({
        protocol: PROTOCOL_VERSION,
        type: "response",
        id: data.id,
        ok: true,
        value: application!.runtime.getState(state),
      });
      return;
    }
    if (data.type === "save") {
      post({
        protocol: PROTOCOL_VERSION,
        type: "response",
        id: data.id,
        ok: true,
        value: application!.runtime.save(state.document),
      });
      return;
    }
    if (data.type === "save.data") {
      post({
        protocol: PROTOCOL_VERSION,
        type: "response",
        id: data.id,
        ok: true,
        value: journalSaveData(journal!, application!.compiled.source),
      });
      return;
    }
    if (data.type === "view.viewport") {
      const device = fxNodeDevicePixels(data.viewport.width, data.viewport.height, data.viewport.dpr),
        aggregate = [...views.values()].reduce(
          (sum, item) => sum + (item === view ? 0 : item.deviceWidth * item.deviceHeight),
          0,
        );
      if (!device || aggregate + device.width * device.height > FXNODE_VIEW_LIMITS.maxActiveDevicePixels) {
        reject(data.id, { code: "view.limit", message: "View rendering limits exceeded" });
        return;
      }
      if (data.expectedSurfaceGeneration !== view!.surfaceGeneration + 1) {
        reject(data.id, { code: "view.surface-generation", message: "Unexpected view surface generation" });
        return;
      }
      if (
        data.hostGeneration < view!.hostGeneration ||
        (data.pointerFence?.before && data.pointerFence.before.hostGeneration > data.hostGeneration)
      ) {
        reject(data.id, { code: "view.host-generation", message: "Stale view host generation" });
        return;
      }
      applyPointerFence(view!, data.pointerFence);
      if (!adoptHostGeneration(view!, data.hostGeneration)) {
        reject(data.id, { code: "view.host-generation", message: "Stale view host generation" });
        return;
      }
      view!.resizePending = true;
      try {
        const mutation = await atlas.resize(view!.id, device);
        view!.viewport = data.viewport;
        view!.deviceWidth = device.width;
        view!.deviceHeight = device.height;
        view!.surfaceGeneration = data.expectedSurfaceGeneration;
        const invalidated = new Set(mutation.invalidatedViewIds);
        invalidated.add(view!.id);
        for (const id of invalidated) views.get(id)?.scheduler.request(0, DirtyReason.Viewport);
        refreshLayout(view!);
        post({ protocol: PROTOCOL_VERSION, type: "response", id: data.id, ok: true });
      } catch (error) {
        if (error instanceof ViewAtlasError && !error.fatal) {
          for (const item of views.values()) item.scheduler.request(0, DirtyReason.Viewport);
          reject(data.id, { code: error.code, message: error.message });
        } else throw error;
      } finally {
        view!.resizePending = false;
      }
      return;
    }
    if (data.type === "view.render") {
      applyPointerFence(view!, data.pointerFence);
      if (!adoptHostGeneration(view!, data.hostGeneration)) return;
      view!.scheduler.request(data.renderId, DirtyReason.Barrier);
      return;
    }
    if (data.type === "view.frame.consumed") {
      releaseFramePixels(view!, data.frameId);
      view!.scheduler.consumed(data.frameId);
      return;
    }
    if (data.type === "view.pointer.flush") {
      applyPointerFence(view!, data.pointerFence);
      return;
    }
    if (data.type === "dispose") {
      for (const item of views.values()) {
        item.scheduler.stop();
        releaseFramePixels(item);
      }
      views.clear();
      atlas.dispose();
      closeResourceImages();
      scope.close();
      return;
    }
    if (data.type === "view.input") {
      applyPointerFence(view!, data.pointerFence);
      if (!adoptHostGeneration(view!, data.hostGeneration)) return;
      input(view!, data.event, data.nodeMenuRequestId, data.resourceOpenRequestId);
    }
  } catch (error) {
    if ((data.type === "init" || data.type === "composition.update") && error instanceof FxNodeCompositionError) {
      reject(data.id, { code: "composition.invalid", message: error.message, issues: error.issues });
      return;
    }
    fatal(error);
  }
}

let viewLifecycleQueue: Promise<void> = Promise.resolve();
const pendingViewLifecycle = new Map<string, number>();
scope.onmessage = (event) => {
  const type =
      typeof event.data === "object" && event.data !== null ? (event.data as { type?: unknown }).type : undefined,
    viewId =
      typeof event.data === "object" &&
      event.data !== null &&
      typeof (event.data as { viewId?: unknown }).viewId === "string"
        ? (event.data as { viewId: string }).viewId
        : undefined;
  if (type === "view.attach" || type === "view.viewport" || type === "view.detach") {
    if (viewId) pendingViewLifecycle.set(viewId, (pendingViewLifecycle.get(viewId) ?? 0) + 1);
    viewLifecycleQueue = viewLifecycleQueue
      .then(() => handleMessage(event))
      .catch(fatal)
      .finally(() => {
        if (!viewId) return;
        const remaining = (pendingViewLifecycle.get(viewId) ?? 1) - 1;
        if (remaining) pendingViewLifecycle.set(viewId, remaining);
        else pendingViewLifecycle.delete(viewId);
      });
  } else if (viewId && pendingViewLifecycle.has(viewId))
    void viewLifecycleQueue.then(() => handleMessage(event)).catch(fatal);
  else void handleMessage(event).catch(fatal);
};
