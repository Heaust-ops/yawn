import type { LinkId, NodeId, ParameterValue, SocketId, Vec2 } from "../core/types.js";
import type { FxNodeHexColor, FxNodeValueSchema } from "../composition/types.js";

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}
export interface ViewTransform {
  readonly center: Vec2;
  readonly zoom: number;
  readonly viewport: Vec2;
  readonly dpr: number;
}
/** View-space (+Y down) modal color-picker geometry. */
export interface ColorPickerLayout {
  readonly bounds: Rect;
  readonly confirm: Rect;
  readonly plane: Rect;
  readonly lightness: Rect;
  readonly alpha: Rect;
  readonly rgba: readonly [Rect, Rect, Rect, Rect];
  readonly hsv: readonly [Rect, Rect, Rect];
  readonly hex: Rect;
}
export interface LayoutSocket {
  readonly id: SocketId;
  readonly nodeId: NodeId;
  readonly label: string;
  readonly dataType: string;
  readonly color: FxNodeHexColor;
  readonly wildcardInput: boolean;
  readonly direction: "input" | "output";
  readonly accepts: readonly string[];
  readonly capacity: number;
  readonly linkIds: readonly LinkId[];
  readonly anchor: Vec2;
  readonly linked: boolean;
}
export type LayoutControlKind =
  | "number"
  | "boolean"
  | "enum"
  | "string"
  | "resource"
  | "vector"
  | "color"
  | "color-ramp"
  | "readonly-json";
export interface LayoutSubfield {
  readonly index: number;
  readonly label: "X" | "Y" | "Z" | "R" | "G" | "B" | "A";
  readonly bounds: Rect;
}
export interface LayoutNumericField {
  readonly component: number;
  readonly bounds: Rect;
  readonly value: Rect;
  readonly decrement: Rect;
  readonly increment: Rect;
  /** Preferred display range for Blender-style proportional fill. */
  readonly range?: { readonly minimum: number; readonly maximum: number };
}
export interface LayoutControl {
  readonly id: string;
  readonly nodeId: NodeId;
  readonly source: "parameter" | "socket-default" | "unknown";
  readonly key: string;
  readonly label: string;
  readonly kind: LayoutControlKind;
  readonly value: ParameterValue | unknown;
  readonly schema?: FxNodeValueSchema;
  readonly resourceId?: string;
  readonly resourceReferencePrefix?: string;
  readonly openTitle?: string;
  readonly linked: boolean;
  readonly bounds: Rect;
  readonly subfields: readonly LayoutSubfield[];
  /** Authoritative numeric value and step-button geometry, in world coordinates. */
  readonly numericFields: readonly LayoutNumericField[];
  /** Authoritative Color Ramp sub-control geometry, in world coordinates. */
  readonly rampBounds?: {
    readonly toolbar: Rect;
    readonly mode: Rect;
    readonly interpolation: Rect;
    readonly hue: Rect;
    readonly gradient: Rect;
    readonly handles: Rect;
    readonly selector: Rect;
    readonly position: Rect;
    readonly color: Rect;
  };
  /** Authoritative inline Oklch grading-wheel geometry, in world coordinates. */
  readonly colorWheelBounds?: { readonly plane: Rect; readonly lightness: Rect };
  /** Resource thumbnail and chooser button geometry, in world coordinates. */
  readonly resourceBounds?: { readonly preview: Rect; readonly open: Rect };
}
export type LayoutRow =
  | { readonly kind: "control"; readonly controlId: string; readonly units: number; readonly bounds: Rect }
  | {
      readonly kind: "grading-wheels";
      readonly wheels: readonly [
        {
          readonly label: string;
          readonly labelBounds: Rect;
          readonly scalarControlId: string;
          readonly colorControlId: string;
        },
        {
          readonly label: string;
          readonly labelBounds: Rect;
          readonly scalarControlId: string;
          readonly colorControlId: string;
        },
        {
          readonly label: string;
          readonly labelBounds: Rect;
          readonly scalarControlId: string;
          readonly colorControlId: string;
        },
      ];
      readonly units: number;
      readonly bounds: Rect;
    }
  | {
      readonly kind: "socket";
      readonly socketId: SocketId;
      readonly controlId?: string;
      readonly units: number;
      readonly bounds: Rect;
    }
  | {
      readonly kind: "header" | "category" | "section" | "panel" | "placeholder";
      readonly label: string;
      readonly units: number;
      readonly bounds: Rect;
    };
export interface LayoutNode {
  readonly id: NodeId;
  readonly parentId?: NodeId;
  readonly typeId: string;
  readonly label: string;
  readonly styleId?: string;
  readonly headerColor: FxNodeHexColor;
  readonly kind: "node" | "frame" | "reroute";
  readonly localPosition: Vec2;
  readonly worldPosition: Vec2;
  readonly authoredSize: Vec2;
  /** Smallest effective expanded size that can display this node's built-in controls at full scale. */
  readonly minimumSize: Vec2;
  readonly bounds: Rect;
  readonly header: Rect;
  readonly collapseHitRect: Rect;
  readonly resizeHitRect: Rect;
  readonly collapsed: boolean;
  readonly muted: boolean;
  readonly visible: boolean;
  readonly rows: readonly LayoutRow[];
  readonly bypasses: readonly { readonly from: Vec2; readonly to: Vec2 }[];
}
export interface LayoutLink {
  readonly id: LinkId;
  readonly fromNodeId: NodeId;
  readonly fromSocketId: SocketId;
  readonly toNodeId: NodeId;
  readonly toSocketId: SocketId;
  readonly dataType: string;
  readonly color: FxNodeHexColor;
  readonly points: readonly Vec2[];
  readonly controls: readonly [Vec2, Vec2];
  readonly bounds: Rect;
  readonly visible: boolean;
  readonly muted: boolean;
}
export interface LayoutSnapshot {
  readonly nodes: ReadonlyMap<NodeId, LayoutNode>;
  readonly sockets: ReadonlyMap<SocketId, LayoutSocket>;
  readonly controls: ReadonlyMap<string, LayoutControl>;
  readonly links: ReadonlyMap<LinkId, LayoutLink>;
  readonly drawOrder: readonly NodeId[];
  readonly graphBounds: Rect;
  readonly transform: ViewTransform;
}
export interface LayoutScene extends Omit<LayoutSnapshot, "transform"> {
  readonly nodeRanks: ReadonlyMap<NodeId, number>;
  readonly linkRanks: ReadonlyMap<LinkId, number>;
}
export interface LayoutView extends LayoutSnapshot {
  readonly candidateNodeIds: readonly NodeId[];
  readonly candidateLinkIds: readonly LinkId[];
  readonly totalNodes: number;
  readonly totalLinks: number;
}

export function layoutSocketsCompatible(from: LayoutSocket, to: LayoutSocket): boolean {
  return (
    from.direction === "output" && to.direction === "input" && (to.wildcardInput || to.accepts.includes(from.dataType))
  );
}
