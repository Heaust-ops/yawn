import type { FxNodeCompositionData, NodeTypeId } from "../composition/types.js";

declare const brand: unique symbol;
export type NodeId = string & { readonly [brand]: "NodeId" };
export type LinkId = string & { readonly [brand]: "LinkId" };
export type SocketId = string & { readonly [brand]: "SocketId" };
export type CommandId = string & { readonly [brand]: "CommandId" };
export type GraphId = string & { readonly [brand]: "GraphId" };
export type JsonValue = null | boolean | number | string | readonly JsonValue[] | { readonly [key: string]: JsonValue };
export interface Vec2 {
  readonly x: number;
  readonly y: number;
}
export type SocketDataType = string;
export type ParameterValue =
  | { readonly kind: "number"; readonly value: number }
  | { readonly kind: "boolean"; readonly value: boolean }
  | { readonly kind: "string"; readonly value: string }
  | { readonly kind: "vector"; readonly value: readonly [number, number, number] }
  | { readonly kind: "color"; readonly value: readonly [number, number, number, number] }
  | { readonly kind: "json"; readonly value: JsonValue };

export interface Socket {
  readonly id: SocketId;
  readonly key: string;
  readonly label: string;
  readonly direction: "input" | "output";
  readonly dataType: SocketDataType;
  readonly accepts: readonly SocketDataType[];
  readonly maxIncomingLinks: number;
  readonly defaultValue?: ParameterValue | null | undefined;
  readonly visible: boolean;
  readonly metadata?: Readonly<Record<string, JsonValue>>;
}
export interface NodeBase {
  readonly id: NodeId;
  readonly typeId: string;
  readonly typeVersion: number;
  /** Upper-left origin. Child positions are local to their parent frame. +Y is up. */
  readonly position: Vec2;
  /** Positive logical dimensions. */
  readonly size: Vec2;
  readonly label: string;
  readonly parameters: Readonly<Record<string, ParameterValue | JsonValue>>;
  readonly sockets: readonly Socket[];
  readonly muted: boolean;
  readonly collapsed: boolean;
  readonly parentId?: NodeId | undefined;
  readonly extensions: Readonly<Record<string, JsonValue>>;
}
export interface KnownNode<C extends FxNodeCompositionData = FxNodeCompositionData> extends NodeBase {
  readonly known: true;
  readonly typeId: NodeTypeId<C>;
  readonly parameters: Readonly<Record<string, ParameterValue>>;
}
export interface UnknownNode extends NodeBase {
  readonly known: false;
}
export type GraphNode<C extends FxNodeCompositionData = FxNodeCompositionData> = KnownNode<C> | UnknownNode;
export interface GraphLink {
  readonly id: LinkId;
  readonly fromNodeId: NodeId;
  readonly fromSocketId: SocketId;
  readonly toNodeId: NodeId;
  readonly toSocketId: SocketId;
  readonly muted: boolean;
  readonly extensions: Readonly<Record<string, JsonValue>>;
}
export interface GraphDocument<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly schemaVersion: 2;
  readonly graphId: GraphId;
  readonly catalogVersion: number;
  readonly nodes: Readonly<Record<string, GraphNode<C>>>;
  readonly links: Readonly<Record<string, GraphLink>>;
  readonly metadata: Readonly<Record<string, JsonValue>>;
}

export interface GraphLayoutNodeV1 extends Omit<NodeBase, "known"> {
  readonly parameters: Readonly<Record<string, ParameterValue | JsonValue>>;
}
export interface GraphLinkV1 extends Omit<GraphLink, "muted"> {}
export interface GraphLayoutV1 {
  readonly schemaVersion: 1;
  readonly graphId: GraphId;
  readonly catalogVersion: number;
  readonly nodes: readonly GraphLayoutNodeV1[];
  readonly links: readonly GraphLinkV1[];
  readonly metadata: Readonly<Record<string, JsonValue>>;
}
export interface GraphLayoutV2 {
  readonly schemaVersion: 2;
  readonly graphId: GraphId;
  readonly catalogVersion: number;
  readonly nodes: readonly GraphLayoutNodeV1[];
  readonly links: readonly GraphLink[];
  readonly metadata: Readonly<Record<string, JsonValue>>;
}
export interface GraphState<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly graphId: GraphId;
  readonly catalogVersion: number;
  readonly nodes: readonly GraphNode<C>[];
  readonly links: readonly GraphLink[];
  readonly metadata: Readonly<Record<string, JsonValue>>;
}
export interface GraphSnapshot<C extends FxNodeCompositionData = FxNodeCompositionData> extends GraphState<C> {
  readonly version: number;
}

export const nodeId = (value: string): NodeId => value as NodeId;
export const linkId = (value: string): LinkId => value as LinkId;
export const socketId = (value: string): SocketId => value as SocketId;
export const commandId = (value: string): CommandId => value as CommandId;
export const graphId = (value: string): GraphId => value as GraphId;
