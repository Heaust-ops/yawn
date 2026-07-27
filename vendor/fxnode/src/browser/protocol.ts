import type { Command, FxNodeSaveData } from "../commands/types.js";
import type { GraphLayoutV2, GraphSnapshot } from "../core/types.js";
import type {
  MutationEnvelope as BoundMutationEnvelope,
  SnapshotEnvelope as BoundSnapshotEnvelope,
} from "../composition/bound-engine.js";
import type { FxNodeCompositionData } from "../composition/types.js";
import type { PointerLaneSnapshot } from "./pointer-lane.js";
import { FXNODE_COMPOSITION_LIMITS } from "../composition/validate.js";
import { validCommand } from "../commands/validate.js";
import type { FxNodeCamera, FxNodeSelectionSnapshot, FxNodeViewport } from "./host-types.js";
import { fxNodeDevicePixels, FXNODE_VIEW_LIMITS } from "./view-limits.js";

export const PROTOCOL_VERSION = 3 as const;
export type ViewId = string;
export type Camera = FxNodeCamera;
export type VersionExpectation = { readonly kind: "current" } | { readonly kind: "exact"; readonly version: number };
export type CompositionRevisionExpectation =
  | { readonly kind: "current" }
  | { readonly kind: "exact"; readonly revision: number };
export type CompositionUpdateWire =
  | { readonly kind: "composition.load"; readonly composition: unknown }
  | { readonly kind: "theme.set"; readonly theme: unknown }
  | { readonly kind: "header-styles.set"; readonly styles: unknown }
  | { readonly kind: "compatibility.set"; readonly compatibility: unknown }
  | { readonly kind: "socket.compose"; readonly id: string; readonly definition: unknown }
  | { readonly kind: "socket.remove"; readonly id: string }
  | { readonly kind: "node.compose"; readonly id: string; readonly definition: unknown }
  | { readonly kind: "node.remove"; readonly id: string };
export type CompositionChange =
  | { readonly kind: "composition.load" }
  | { readonly kind: "theme.set" }
  | { readonly kind: "header-styles.set" }
  | { readonly kind: "compatibility.set" }
  | { readonly kind: "socket.compose" | "socket.remove" | "node.compose" | "node.remove"; readonly id: string };
export type CompositionReceipt =
  | {
      readonly status: "committed";
      readonly revision: number;
      readonly graphVersion: number;
      readonly graphChanged: boolean;
      readonly historyReset: true;
    }
  | {
      readonly status: "noop";
      readonly revision: number;
      readonly graphVersion: number;
      readonly graphChanged: false;
      readonly historyReset: false;
    };
export interface CompositionChangeEnvelope {
  readonly baseRevision: number;
  readonly revision: number;
  readonly change: CompositionChange;
  readonly baseGraphVersion: number;
  readonly graphVersion: number;
  readonly graphChanged: boolean;
  readonly historyReset: true;
}
export type WorkerRequest =
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "init";
      id: string;
      applicationId: unknown;
      applicationVersion: unknown;
      resources: unknown;
      historyLimit: number;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.attach";
      id: string;
      viewId: ViewId;
      viewport: Viewport;
      camera: Camera;
      pointerLane?: SharedArrayBuffer;
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "view.detach"; id: string; viewId: string }
  | { protocol: typeof PROTOCOL_VERSION; type: "command"; id: string; command: Command; expected: VersionExpectation }
  | { protocol: typeof PROTOCOL_VERSION; type: "load"; id: string; data: unknown; expected: VersionExpectation }
  | { protocol: typeof PROTOCOL_VERSION; type: "state.get" | "save" | "save.data"; id: string }
  | { protocol: typeof PROTOCOL_VERSION; type: "state.set"; id: string; state: unknown; expected: VersionExpectation }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "composition.update";
      id: string;
      expected: CompositionRevisionExpectation;
      update: CompositionUpdateWire;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.viewport";
      id: string;
      viewId: string;
      viewport: Viewport;
      expectedSurfaceGeneration: number;
      hostGeneration: number;
      pointerFence?: PointerFence;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.render";
      viewId: string;
      renderId: number;
      hostGeneration: number;
      pointerFence?: PointerFence;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.input";
      viewId: string;
      event: InputEventWire;
      hostGeneration: number;
      pointerFence?: PointerFence;
      nodeMenuRequestId?: string;
      resourceOpenRequestId?: string;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.node.add";
      viewId: string;
      id: string;
      nodeId: string;
      typeId: string;
      viewPosition: { x: number; y: number };
      expected: VersionExpectation;
      pointerFence?: PointerFence;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.selection.remove";
      viewId: string;
      id: string;
      expected: VersionExpectation;
      pointerFence?: PointerFence;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.selection.mute";
      viewId: string;
      id: string;
      value: boolean;
      expected: VersionExpectation;
      pointerFence?: PointerFence;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.resource.set";
      viewId: string;
      id: string;
      authorization: { viewId: string; token: string; graphVersion: number; compositionRevision: number };
      resource: { name: string; mime: string; bytes: ArrayBuffer };
      expected: VersionExpectation;
      pointerFence?: PointerFence;
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "view.pointer.flush"; viewId: string; pointerFence: PointerFence }
  | { protocol: typeof PROTOCOL_VERSION; type: "view.frame.consumed"; viewId: string; frameId: number }
  | { protocol: typeof PROTOCOL_VERSION; type: "dispose" };
export type Viewport = FxNodeViewport;
export interface PointerFence {
  readonly generation: number;
  readonly before?: PointerLaneSnapshot;
}
export type InputEventWire =
  | {
      kind: "pointer";
      phase: "down" | "move" | "up" | "cancel";
      pointerId: number;
      pointerType: string;
      position: { x: number; y: number };
      button: number;
      buttons: number;
      modifiers: number;
    }
  | { kind: "wheel"; position: { x: number; y: number }; delta: { x: number; y: number }; modifiers: number }
  | { kind: "key"; phase: "down" | "up"; key: string; code: string; repeat: boolean; modifiers: number }
  | { kind: "focus"; phase: "focus" | "blur" }
  | { kind: "outside-pointer"; button: number };
export interface HostInteractionSnapshot {
  readonly colorPickerOpen: boolean;
}
export type WorkerMessage =
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "response";
      id: string;
      ok: true;
      value?:
        | GraphSnapshot
        | GraphLayoutV2
        | FxNodeSaveData
        | { status: "committed" | "noop"; version: number }
        | CompositionReceipt;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "response";
      id: string;
      ok: false;
      error: { code: string; message: string; path?: string; issues?: unknown };
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "mutation"; envelope: BoundMutationEnvelope<FxNodeCompositionData> }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "snapshot.event";
      envelope: BoundSnapshotEnvelope<FxNodeCompositionData>;
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "composition.event"; envelope: CompositionChangeEnvelope }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.selection.host";
      viewId: string;
      projection: FxNodeSelectionSnapshot;
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.frame";
      viewId: string;
      bitmap: ImageBitmap;
      renderId: number;
      frameId: number;
      hostGeneration: number;
      surfaceGeneration: number;
      deviceWidth: number;
      deviceHeight: number;
      host: HostInteractionSnapshot;
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "view.node-menu.result"; viewId: string; requestId: string; open: false }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.node-menu.result";
      viewId: string;
      requestId: string;
      open: true;
      compositionRevision: number;
      viewPosition: { x: number; y: number };
    }
  | {
      protocol: typeof PROTOCOL_VERSION;
      type: "view.resource.open";
      viewId: string;
      requestId: string;
      authorization: { viewId: string; token: string; graphVersion: number; compositionRevision: number };
      resource: {
        id: string;
        kind: "image";
        title: string;
        openTitle: string;
        accept: readonly string[];
        maxBytes: number;
        maxWidth: number;
        maxHeight: number;
        maxPixels: number;
      };
    }
  | { protocol: typeof PROTOCOL_VERSION; type: "fatal"; error: { code: string; message: string } };

const record = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const exact = (v: Record<string, unknown>, keys: readonly string[]): boolean =>
  Object.keys(v).length === keys.length && keys.every((k) => Object.hasOwn(v, k));
const nonnegativeSafeInteger = (value: unknown): value is number =>
  Number.isSafeInteger(value) && (value as number) >= 0;
const forbiddenCompositionIds = new Set(["__proto__", "prototype", "constructor"]);
const compositionId = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= FXNODE_COMPOSITION_LIMITS.maxIdLength &&
  !/[\u0000-\u001f\u007f]/.test(value) &&
  !forbiddenCompositionIds.has(value);
function validCompositionExpectation(value: unknown): value is CompositionRevisionExpectation {
  return (
    record(value) &&
    ((value.kind === "current" && exact(value, ["kind"])) ||
      (value.kind === "exact" && exact(value, ["kind", "revision"]) && nonnegativeSafeInteger(value.revision)))
  );
}
function validCompositionUpdate(value: unknown): value is CompositionUpdateWire {
  if (!record(value)) return false;
  if (value.kind === "composition.load") return exact(value, ["kind", "composition"]);
  if (value.kind === "theme.set") return exact(value, ["kind", "theme"]);
  if (value.kind === "header-styles.set") return exact(value, ["kind", "styles"]);
  if (value.kind === "compatibility.set") return exact(value, ["kind", "compatibility"]);
  if (value.kind === "socket.compose" || value.kind === "node.compose")
    return exact(value, ["kind", "id", "definition"]) && compositionId(value.id);
  if (value.kind === "socket.remove" || value.kind === "node.remove")
    return exact(value, ["kind", "id"]) && compositionId(value.id);
  return false;
}
function validCompositionChange(value: unknown): value is CompositionChange {
  if (!record(value)) return false;
  if (
    value.kind === "theme.set" ||
    value.kind === "header-styles.set" ||
    value.kind === "composition.load" ||
    value.kind === "compatibility.set"
  )
    return exact(value, ["kind"]);
  return (
    (value.kind === "socket.compose" ||
      value.kind === "socket.remove" ||
      value.kind === "node.compose" ||
      value.kind === "node.remove") &&
    exact(value, ["kind", "id"]) &&
    compositionId(value.id)
  );
}
function validCompositionReceiptUnsafe(value: unknown): value is CompositionReceipt {
  if (
    !record(value) ||
    !exact(value, ["status", "revision", "graphVersion", "graphChanged", "historyReset"]) ||
    !nonnegativeSafeInteger(value.revision) ||
    !nonnegativeSafeInteger(value.graphVersion)
  )
    return false;
  if (value.status === "committed")
    return value.revision > 0 && typeof value.graphChanged === "boolean" && value.historyReset === true;
  return value.status === "noop" && value.graphChanged === false && value.historyReset === false;
}
export function validCompositionReceipt(value: unknown): value is CompositionReceipt {
  try {
    return validCompositionReceiptUnsafe(value);
  } catch {
    return false;
  }
}
export function validCommandReceipt(
  value: unknown,
): value is { readonly status: "committed" | "noop"; readonly version: number } {
  try {
    return (
      record(value) &&
      exact(value, ["status", "version"]) &&
      (value.status === "committed" || value.status === "noop") &&
      nonnegativeSafeInteger(value.version)
    );
  } catch {
    return false;
  }
}
function validCompositionEnvelope(value: unknown): value is CompositionChangeEnvelope {
  return (
    record(value) &&
    exact(value, [
      "baseRevision",
      "revision",
      "change",
      "baseGraphVersion",
      "graphVersion",
      "graphChanged",
      "historyReset",
    ]) &&
    nonnegativeSafeInteger(value.baseRevision) &&
    nonnegativeSafeInteger(value.revision) &&
    value.revision === value.baseRevision + 1 &&
    nonnegativeSafeInteger(value.baseGraphVersion) &&
    nonnegativeSafeInteger(value.graphVersion) &&
    typeof value.graphChanged === "boolean" &&
    value.historyReset === true &&
    validCompositionChange(value.change) &&
    value.graphVersion === value.baseGraphVersion + (value.graphChanged ? 1 : 0)
  );
}
const boundedString = (value: unknown, max: number, nonempty = false): value is string =>
  typeof value === "string" && (!nonempty || value.length > 0) && value.length <= max;
function validHostResourcePolicy(
  value: unknown,
): value is Extract<WorkerMessage, { type: "view.resource.open" }>["resource"] {
  if (
    !record(value) ||
    !exact(value, ["id", "kind", "title", "openTitle", "accept", "maxBytes", "maxWidth", "maxHeight", "maxPixels"]) ||
    !compositionId(value.id) ||
    value.kind !== "image" ||
    !boundedString(value.title, FXNODE_COMPOSITION_LIMITS.maxTitleLength) ||
    !boundedString(value.openTitle, FXNODE_COMPOSITION_LIMITS.maxTitleLength) ||
    !Array.isArray(value.accept)
  )
    return false;
  if (
    !nonnegativeSafeInteger(value.maxBytes) ||
    value.maxBytes === 0 ||
    value.maxBytes > FXNODE_COMPOSITION_LIMITS.maxImageBytes ||
    !nonnegativeSafeInteger(value.maxWidth) ||
    value.maxWidth === 0 ||
    value.maxWidth > FXNODE_COMPOSITION_LIMITS.maxImageDimension ||
    !nonnegativeSafeInteger(value.maxHeight) ||
    value.maxHeight === 0 ||
    value.maxHeight > FXNODE_COMPOSITION_LIMITS.maxImageDimension ||
    !nonnegativeSafeInteger(value.maxPixels) ||
    value.maxPixels === 0 ||
    value.maxPixels > FXNODE_COMPOSITION_LIMITS.maxImagePixels ||
    value.maxPixels > value.maxWidth * value.maxHeight
  )
    return false;
  const accepted = new Set<string>();
  for (const item of value.accept) {
    if (!boundedString(item, FXNODE_COMPOSITION_LIMITS.maxKeywordLength, true) || accepted.has(item)) return false;
    accepted.add(item);
  }
  return value.accept.length > 0;
}
function validViewportUnsafe(v: unknown): v is Viewport {
  return (
    record(v) &&
    exact(v, ["width", "height", "dpr"]) &&
    typeof v.width === "number" &&
    Number.isFinite(v.width) &&
    v.width >= 0 &&
    v.width <= FXNODE_VIEW_LIMITS.maxLogicalDimension &&
    typeof v.height === "number" &&
    Number.isFinite(v.height) &&
    v.height >= 0 &&
    v.height <= FXNODE_VIEW_LIMITS.maxLogicalDimension &&
    typeof v.dpr === "number" &&
    Number.isFinite(v.dpr) &&
    v.dpr > 0 &&
    v.dpr <= FXNODE_VIEW_LIMITS.maxDpr &&
    !!fxNodeDevicePixels(v.width, v.height, v.dpr)
  );
}
export function validViewport(v: unknown): v is Viewport {
  try {
    return validViewportUnsafe(v);
  } catch {
    return false;
  }
}
function validCameraUnsafe(v: unknown): v is FxNodeCamera {
  return (
    record(v) &&
    exact(v, ["center", "zoom"]) &&
    finitePoint(v.center) &&
    typeof v.zoom === "number" &&
    Number.isFinite(v.zoom) &&
    v.zoom >= FXNODE_VIEW_LIMITS.minZoom &&
    v.zoom <= FXNODE_VIEW_LIMITS.maxZoom
  );
}
/** Total camera validator for worker-bound hostile values. */
export function validCamera(v: unknown): v is FxNodeCamera {
  try {
    return validCameraUnsafe(v);
  } catch {
    return false;
  }
}
function validRequestUnsafe(v: unknown): v is WorkerRequest {
  if (!record(v) || v.protocol !== PROTOCOL_VERSION || typeof v.type !== "string") return false;
  if (v.type.startsWith("view.") && !id(v.viewId)) return false;
  if (v.type === "init")
    return (
      Object.keys(v).every((k) =>
        ["protocol", "type", "id", "applicationId", "applicationVersion", "resources", "historyLimit"].includes(k),
      ) &&
      ["protocol", "type", "id", "applicationId", "applicationVersion", "resources", "historyLimit"].every((k) =>
        Object.hasOwn(v, k),
      ) &&
      id(v.id) &&
      typeof v.applicationId === "string" &&
      Number.isSafeInteger(v.applicationVersion) &&
      Number.isSafeInteger(v.historyLimit) &&
      (v.historyLimit as number) >= 0 &&
      (v.historyLimit as number) <= 1000
    );
  if (v.type === "view.attach")
    return (
      Object.keys(v).every((key) =>
        ["protocol", "type", "id", "viewId", "viewport", "camera", "pointerLane"].includes(key),
      ) &&
      ["protocol", "type", "id", "viewId", "viewport", "camera"].every((key) => Object.hasOwn(v, key)) &&
      id(v.id) &&
      validViewport(v.viewport) &&
      validCamera(v.camera) &&
      (v.pointerLane === undefined ||
        (typeof SharedArrayBuffer === "function" && v.pointerLane instanceof SharedArrayBuffer))
    );
  if (v.type === "view.detach") return exact(v, ["protocol", "type", "id", "viewId"]) && id(v.id);
  if (v.type === "command")
    return (
      exact(v, ["protocol", "type", "id", "command", "expected"]) &&
      id(v.id) &&
      validCommand(v.command) &&
      validExpectation(v.expected)
    );
  if (v.type === "load")
    return exact(v, ["protocol", "type", "id", "data", "expected"]) && id(v.id) && validExpectation(v.expected);
  if (v.type === "state.get" || v.type === "save" || v.type === "save.data")
    return exact(v, ["protocol", "type", "id"]) && id(v.id);
  // State is deliberately opaque here. The composition-bound worker authority decodes it.
  if (v.type === "state.set")
    return exact(v, ["protocol", "type", "id", "state", "expected"]) && id(v.id) && validExpectation(v.expected);
  if (v.type === "composition.update")
    return (
      exact(v, ["protocol", "type", "id", "expected", "update"]) &&
      id(v.id) &&
      validCompositionExpectation(v.expected) &&
      validCompositionUpdate(v.update)
    );
  if (v.type === "view.viewport")
    return (
      Object.keys(v).every((key) =>
        [
          "protocol",
          "type",
          "id",
          "viewId",
          "viewport",
          "expectedSurfaceGeneration",
          "hostGeneration",
          "pointerFence",
        ].includes(key),
      ) &&
      ["protocol", "type", "id", "viewId", "viewport", "expectedSurfaceGeneration", "hostGeneration"].every((key) =>
        Object.hasOwn(v, key),
      ) &&
      id(v.id) &&
      validViewport(v.viewport) &&
      nonnegativeSafeInteger(v.expectedSurfaceGeneration) &&
      nonnegativeSafeInteger(v.hostGeneration) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  if (v.type === "view.render")
    return (
      Object.keys(v).every((key) =>
        ["protocol", "type", "viewId", "renderId", "hostGeneration", "pointerFence"].includes(key),
      ) &&
      ["protocol", "type", "viewId", "renderId", "hostGeneration"].every((key) => Object.hasOwn(v, key)) &&
      nonnegativeSafeInteger(v.renderId) &&
      nonnegativeSafeInteger(v.hostGeneration) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  if (v.type === "view.frame.consumed")
    return exact(v, ["protocol", "type", "viewId", "frameId"]) && nonnegativeSafeInteger(v.frameId);
  if (v.type === "dispose") return exact(v, ["protocol", "type"]);
  if (v.type === "view.pointer.flush")
    return exact(v, ["protocol", "type", "viewId", "pointerFence"]) && validPointerFence(v.pointerFence);
  if (v.type === "view.node.add")
    return (
      Object.keys(v).every((k) =>
        ["protocol", "type", "viewId", "id", "nodeId", "typeId", "viewPosition", "expected", "pointerFence"].includes(
          k,
        ),
      ) &&
      ["protocol", "type", "viewId", "id", "nodeId", "typeId", "viewPosition", "expected"].every((k) =>
        Object.hasOwn(v, k),
      ) &&
      id(v.id) &&
      id(v.nodeId) &&
      nodeTypeId(v.typeId) &&
      finitePoint(v.viewPosition) &&
      validExpectation(v.expected) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  if (v.type === "view.selection.remove")
    return (
      Object.keys(v).every((k) => ["protocol", "type", "viewId", "id", "expected", "pointerFence"].includes(k)) &&
      ["protocol", "type", "viewId", "id", "expected"].every((k) => Object.hasOwn(v, k)) &&
      id(v.id) &&
      validExpectation(v.expected) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  if (v.type === "view.selection.mute")
    return (
      Object.keys(v).every((k) =>
        ["protocol", "type", "viewId", "id", "value", "expected", "pointerFence"].includes(k),
      ) &&
      ["protocol", "type", "viewId", "id", "value", "expected"].every((k) => Object.hasOwn(v, k)) &&
      id(v.id) &&
      typeof v.value === "boolean" &&
      validExpectation(v.expected) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  if (v.type === "view.resource.set")
    return (
      Object.keys(v).every((k) =>
        ["protocol", "type", "viewId", "id", "authorization", "resource", "expected", "pointerFence"].includes(k),
      ) &&
      ["protocol", "type", "viewId", "id", "authorization", "resource", "expected"].every((k) => Object.hasOwn(v, k)) &&
      id(v.id) &&
      record(v.authorization) &&
      exact(v.authorization, ["viewId", "token", "graphVersion", "compositionRevision"]) &&
      id(v.authorization.viewId) &&
      id(v.authorization.token) &&
      nonnegativeSafeInteger(v.authorization.graphVersion) &&
      nonnegativeSafeInteger(v.authorization.compositionRevision) &&
      record(v.resource) &&
      exact(v.resource, ["name", "mime", "bytes"]) &&
      typeof v.resource.name === "string" &&
      v.resource.name.length > 0 &&
      v.resource.name.length <= 255 &&
      !/[\u0000-\u001f\u007f]/.test(v.resource.name) &&
      typeof v.resource.mime === "string" &&
      v.resource.mime.length <= 128 &&
      v.resource.bytes instanceof ArrayBuffer &&
      v.resource.bytes.byteLength > 0 &&
      v.resource.bytes.byteLength <= 32 * 1024 * 1024 &&
      validExpectation(v.expected) &&
      (v.pointerFence === undefined || validPointerFence(v.pointerFence))
    );
  return (
    v.type === "view.input" &&
    Object.keys(v).every((k) =>
      [
        "protocol",
        "type",
        "viewId",
        "event",
        "hostGeneration",
        "pointerFence",
        "nodeMenuRequestId",
        "resourceOpenRequestId",
      ].includes(k),
    ) &&
    ["protocol", "type", "viewId", "event", "hostGeneration"].every((k) => Object.hasOwn(v, k)) &&
    validInput(v.event) &&
    nonnegativeSafeInteger(v.hostGeneration) &&
    (v.pointerFence === undefined || validPointerFence(v.pointerFence)) &&
    (v.nodeMenuRequestId === undefined || id(v.nodeMenuRequestId)) &&
    (v.resourceOpenRequestId === undefined || id(v.resourceOpenRequestId))
  );
}
/** Total across hostile objects (including proxies with throwing traps). */
export function validRequest(v: unknown): v is WorkerRequest {
  try {
    return validRequestUnsafe(v);
  } catch {
    return false;
  }
}
const finitePoint = (v: unknown): boolean =>
  record(v) &&
  exact(v, ["x", "y"]) &&
  typeof v.x === "number" &&
  Number.isFinite(v.x) &&
  typeof v.y === "number" &&
  Number.isFinite(v.y);
function validInput(v: unknown): v is InputEventWire {
  if (!record(v)) return false;
  if (v.kind === "outside-pointer") return exact(v, ["kind", "button"]) && Number.isInteger(v.button);
  if (v.kind === "pointer")
    return (
      exact(v, ["kind", "phase", "pointerId", "pointerType", "position", "button", "buttons", "modifiers"]) &&
      ["down", "move", "up", "cancel"].includes(String(v.phase)) &&
      Number.isSafeInteger(v.pointerId) &&
      typeof v.pointerType === "string" &&
      finitePoint(v.position) &&
      Number.isInteger(v.button) &&
      Number.isInteger(v.buttons) &&
      Number.isInteger(v.modifiers)
    );
  if (v.kind === "wheel")
    return (
      exact(v, ["kind", "position", "delta", "modifiers"]) &&
      finitePoint(v.position) &&
      finitePoint(v.delta) &&
      Number.isInteger(v.modifiers)
    );
  if (v.kind === "key")
    return (
      exact(v, ["kind", "phase", "key", "code", "repeat", "modifiers"]) &&
      ["down", "up"].includes(String(v.phase)) &&
      typeof v.key === "string" &&
      typeof v.code === "string" &&
      typeof v.repeat === "boolean" &&
      Number.isInteger(v.modifiers)
    );
  return v.kind === "focus" && exact(v, ["kind", "phase"]) && (v.phase === "focus" || v.phase === "blur");
}
function validPointerFence(v: unknown): v is PointerFence {
  return (
    record(v) &&
    Object.keys(v).every((k) => ["generation", "before"].includes(k)) &&
    Object.hasOwn(v, "generation") &&
    Number.isInteger(v.generation) &&
    (v.before === undefined ||
      (record(v.before) &&
        exact(v.before, ["sequence", "hostGeneration", "event"]) &&
        Number.isInteger(v.before.sequence) &&
        nonnegativeSafeInteger(v.before.hostGeneration) &&
        validInput(v.before.event) &&
        record(v.before.event) &&
        v.before.event.kind === "pointer" &&
        v.before.event.phase === "move"))
  );
}
const id = (v: unknown): boolean => typeof v === "string" && v.length > 0 && v.length <= 512;
const nodeTypeId = (v: unknown): boolean =>
  typeof v === "string" && v.length > 0 && v.length <= FXNODE_COMPOSITION_LIMITS.maxIdLength;
function validExpectation(v: unknown): v is VersionExpectation {
  return (
    record(v) &&
    ((v.kind === "current" && exact(v, ["kind"])) ||
      (v.kind === "exact" &&
        exact(v, ["kind", "version"]) &&
        Number.isSafeInteger(v.version) &&
        (v.version as number) >= 0))
  );
}
function validWorkerMessageUnsafe(v: unknown): v is WorkerMessage {
  if (!record(v) || v.protocol !== PROTOCOL_VERSION || typeof v.type !== "string") return false;
  if (v.type.startsWith("view.") && !id(v.viewId)) return false;
  if (v.type === "view.frame")
    return (
      exact(v, [
        "protocol",
        "type",
        "viewId",
        "bitmap",
        "renderId",
        "frameId",
        "hostGeneration",
        "surfaceGeneration",
        "deviceWidth",
        "deviceHeight",
        "host",
      ]) &&
      typeof ImageBitmap !== "undefined" &&
      v.bitmap instanceof ImageBitmap &&
      nonnegativeSafeInteger(v.renderId) &&
      nonnegativeSafeInteger(v.frameId) &&
      nonnegativeSafeInteger(v.hostGeneration) &&
      nonnegativeSafeInteger(v.surfaceGeneration) &&
      Number.isSafeInteger(v.deviceWidth) &&
      (v.deviceWidth as number) > 0 &&
      (v.deviceWidth as number) <= FXNODE_VIEW_LIMITS.maxDeviceDimension &&
      Number.isSafeInteger(v.deviceHeight) &&
      (v.deviceHeight as number) > 0 &&
      (v.deviceHeight as number) <= FXNODE_VIEW_LIMITS.maxDeviceDimension &&
      (v.deviceWidth as number) * (v.deviceHeight as number) <= FXNODE_VIEW_LIMITS.maxDevicePixelsPerView &&
      v.bitmap.width === v.deviceWidth &&
      v.bitmap.height === v.deviceHeight &&
      validHostSnapshot(v.host)
    );
  if (v.type === "mutation")
    return exact(v, ["protocol", "type", "envelope"]) && record(v.envelope) && Number.isSafeInteger(v.envelope.version);
  if (v.type === "snapshot.event")
    return exact(v, ["protocol", "type", "envelope"]) && record(v.envelope) && Number.isSafeInteger(v.envelope.version);
  if (v.type === "composition.event")
    return exact(v, ["protocol", "type", "envelope"]) && validCompositionEnvelope(v.envelope);
  if (v.type === "view.selection.host")
    return exact(v, ["protocol", "type", "viewId", "projection"]) && validSelectionProjection(v.projection);
  if (v.type === "view.node-menu.result")
    return v.open === false
      ? exact(v, ["protocol", "type", "viewId", "requestId", "open"]) && id(v.requestId)
      : v.open === true &&
          exact(v, ["protocol", "type", "viewId", "requestId", "open", "compositionRevision", "viewPosition"]) &&
          id(v.requestId) &&
          nonnegativeSafeInteger(v.compositionRevision) &&
          finitePoint(v.viewPosition);
  if (v.type === "view.resource.open")
    return (
      exact(v, ["protocol", "type", "viewId", "requestId", "authorization", "resource"]) &&
      id(v.requestId) &&
      record(v.authorization) &&
      exact(v.authorization, ["viewId", "token", "graphVersion", "compositionRevision"]) &&
      id(v.authorization.viewId) &&
      id(v.authorization.token) &&
      nonnegativeSafeInteger(v.authorization.graphVersion) &&
      nonnegativeSafeInteger(v.authorization.compositionRevision) &&
      validHostResourcePolicy(v.resource)
    );
  if (v.type === "fatal")
    return (
      exact(v, ["protocol", "type", "error"]) &&
      record(v.error) &&
      exact(v.error, ["code", "message"]) &&
      typeof v.error.code === "string" &&
      typeof v.error.message === "string"
    );
  return (
    v.type === "response" &&
    id(v.id) &&
    typeof v.ok === "boolean" &&
    (v.ok
      ? exact(v, ["protocol", "type", "id", "ok"]) || exact(v, ["protocol", "type", "id", "ok", "value"])
      : exact(v, ["protocol", "type", "id", "ok", "error"]) && validError(v.error))
  );
}
/** Total across hostile objects (including proxies with throwing traps). */
export function validWorkerMessage(v: unknown): v is WorkerMessage {
  try {
    return validWorkerMessageUnsafe(v);
  } catch {
    return false;
  }
}
function validError(v: unknown): boolean {
  return (
    record(v) &&
    Object.hasOwn(v, "code") &&
    Object.hasOwn(v, "message") &&
    Object.keys(v).every((k) => ["code", "message", "path", "issues"].includes(k)) &&
    typeof v.code === "string" &&
    v.code.length <= 128 &&
    typeof v.message === "string" &&
    v.message.length <= 2048 &&
    (v.path === undefined || (Object.hasOwn(v, "path") && typeof v.path === "string" && v.path.length <= 512)) &&
    (v.issues === undefined ||
      (Object.hasOwn(v, "issues") &&
        Array.isArray(v.issues) &&
        v.issues.length <= 100 &&
        v.issues.every(
          (issue) =>
            record(issue) &&
            exact(issue, ["code", "path", "message"]) &&
            typeof issue.code === "string" &&
            issue.code.length <= 128 &&
            typeof issue.path === "string" &&
            issue.path.length <= 512 &&
            typeof issue.message === "string" &&
            issue.message.length <= 1024,
        )))
  );
}
function validHostSnapshot(v: unknown): v is HostInteractionSnapshot {
  return record(v) && exact(v, ["colorPickerOpen"]) && typeof v.colorPickerOpen === "boolean";
}
function validSelectionProjection(v: unknown): v is FxNodeSelectionSnapshot {
  if (
    !record(v) ||
    !exact(v, ["nodeCount", "linkCount", "canRemove", "mute"]) ||
    !nonnegativeSafeInteger(v.nodeCount) ||
    !nonnegativeSafeInteger(v.linkCount) ||
    v.canRemove !== (v.nodeCount > 0 || v.linkCount > 0) ||
    !record(v.mute)
  )
    return false;
  return v.mute.enabled === false
    ? exact(v.mute, ["enabled"])
    : v.mute.enabled === true &&
        exact(v.mute, ["enabled", "state"]) &&
        ["all-muted", "all-unmuted", "mixed"].includes(String(v.mute.state));
}
