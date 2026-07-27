import type { LinkId, NodeId, ParameterValue, Vec2 } from "../core/types.js";
import type { ColorRamp } from "../widgets/color-ramp.js";
import type { Oklch, Rgba } from "../color/oklab.js";
import type { ColorPickerLayout } from "../layout/types.js";
export interface DragSession {
  readonly pointerId: number;
  readonly startView: Vec2;
  readonly startWorld: Vec2;
  readonly origins: ReadonlyMap<NodeId, Vec2>;
  moved: boolean;
}
export interface CollapseAnimation {
  from: number;
  to: 0 | 1;
  value: number;
  startedAt: number;
  durationMs: number;
}
export type ControlEdit =
  | { kind: "string"; controlId: string; buffer: string }
  | { kind: "number"; controlId: string; component: number; buffer: string; selectAll: boolean };
export interface WorkerSession {
  knife?: { pointerId: number; points: readonly Vec2[]; crossed: Set<LinkId>; mode: "remove" | "mute" };
}
export interface WorkerSession {
  colorPicker?: {
    layout: ColorPickerLayout;
    controlId: string;
    target: { kind: "control" } | { kind: "ramp-stop"; stopId: string; original: ColorRamp };
    model: Oklch;
    rgba: Rgba;
    hsv: readonly [number, number, number];
    edit?: { field: "rgba" | "hsv" | "hex"; index: number; buffer: string; selectAll: boolean; invalid: boolean };
    drag?: { pointerId: number; region: "plane" | "lightness" | "alpha" };
  };
}
export interface WorkerSession {
  colorWheel?: {
    controlId: string;
    original: ParameterValue;
    model: Oklch;
    rgba: Rgba;
    pointerId: number;
    region: "plane" | "lightness";
    bounds: { x: number; y: number; width: number; height: number };
  };
}
export interface WorkerSession {
  cameraCenter: Vec2;
  zoom: number;
  selectedNodes: Set<NodeId>;
  selectedLinks: Set<LinkId>;
  activeNode?: NodeId;
  hoverNode?: NodeId;
  hoveredControl?: string;
  focusedControl?: string;
  hoveredRampTarget?: string;
  focusedRampTarget?: string;
  activeRampStopByControl: Map<string, string>;
  collapseAnimations: Map<NodeId, CollapseAnimation>;
  controlEdit?: ControlEdit;
  previewValues: Map<string, ParameterValue>;
  scrub?: {
    pointerId: number;
    controlId: string;
    component: number;
    startX: number;
    original: ParameterValue;
    moved: boolean;
  };
  rampDrag?: { pointerId: number; controlId: string; stopId: string; original: ParameterValue };
  reroutePress?: { pointerId: number; nodeId: NodeId; socketId: import("../core/types.js").SocketId; start: Vec2 };
  uiOrder: NodeId[];
  previewPositions: Map<NodeId, Vec2>;
  previewSizes: Map<NodeId, Vec2>;
  pointer?: Vec2;
  drag?: DragSession;
  modalMove?: Omit<DragSession, "pointerId">;
  box?: { pointerId: number; start: Vec2; current: Vec2; checkpoint: Set<NodeId>; add: boolean };
  linkDrag?: {
    pointerId: number;
    from: import("../core/types.js").SocketId;
    current: Vec2;
    candidate?: import("../core/types.js").SocketId;
  };
  resize?: { pointerId: number; id: NodeId };
  parentHighlight?: NodeId;
  pan?: { pointerId: number; last: Vec2 };
}
export const createSession = (
  camera: { readonly center: Vec2; readonly zoom: number } = {
    center: { x: 0, y: 0 },
    zoom: 1,
  },
): WorkerSession => ({
  cameraCenter: { ...camera.center },
  zoom: camera.zoom,
  selectedNodes: new Set(),
  selectedLinks: new Set(),
  activeRampStopByControl: new Map(),
  collapseAnimations: new Map(),
  uiOrder: [],
  previewPositions: new Map(),
  previewSizes: new Map(),
  previewValues: new Map(),
});
function resetDocumentTransients(session: WorkerSession): void {
  delete session.hoverNode;
  delete session.hoveredControl;
  delete session.focusedControl;
  delete session.hoveredRampTarget;
  delete session.focusedRampTarget;
  delete session.knife;
  delete session.drag;
  delete session.scrub;
  delete session.rampDrag;
  delete session.reroutePress;
  delete session.modalMove;
  delete session.box;
  delete session.linkDrag;
  delete session.resize;
  delete session.parentHighlight;
  delete session.pan;
  delete session.controlEdit;
  delete session.colorPicker;
  delete session.colorWheel;
  session.activeRampStopByControl.clear();
  session.collapseAnimations.clear();
  session.previewValues.clear();
  session.previewPositions.clear();
  session.previewSizes.clear();
}
export function resetSessionForGraphReplacement(session: WorkerSession): void {
  session.selectedNodes.clear();
  session.selectedLinks.clear();
  session.uiOrder = [];
  delete session.activeNode;
  resetDocumentTransients(session);
}
export function resetSessionForCompositionRebind(
  session: WorkerSession,
  nodeIds: ReadonlySet<NodeId>,
  linkIds: ReadonlySet<LinkId>,
  retainedUiOrder: readonly NodeId[],
): void {
  session.selectedNodes = new Set([...session.selectedNodes].filter((id) => nodeIds.has(id)));
  session.selectedLinks = new Set([...session.selectedLinks].filter((id) => linkIds.has(id)));
  session.uiOrder = [...retainedUiOrder];
  if (session.activeNode && !nodeIds.has(session.activeNode)) delete session.activeNode;
  resetDocumentTransients(session);
}
