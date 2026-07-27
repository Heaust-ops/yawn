import type { FxNodeCompositionData, NodeTypeId } from "../composition/types.js";
import type {
  CommandId,
  GraphLayoutV2,
  GraphLink,
  LinkId,
  NodeId,
  ParameterValue,
  SocketId,
  Vec2,
} from "../core/types.js";

export type Command<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | { readonly type: "batch"; readonly commands: readonly BatchCommand<C>[] }
  | {
      readonly type: "node.add";
      readonly nodeId: NodeId;
      readonly nodeType: NodeTypeId<C>;
      readonly position: Vec2;
      readonly parentId?: NodeId;
    }
  | { readonly type: "node.remove"; readonly id: NodeId }
  | { readonly type: "node.move"; readonly id: NodeId; readonly position: Vec2 }
  | { readonly type: "node.resize"; readonly id: NodeId; readonly size: Vec2 }
  | { readonly type: "node.label"; readonly id: NodeId; readonly label: string }
  | { readonly type: "node.parameter"; readonly id: NodeId; readonly key: string; readonly value: ParameterValue }
  | { readonly type: "node.parameter-reset"; readonly id: NodeId; readonly key: string }
  | {
      readonly type: "node.socket-default";
      readonly id: NodeId;
      readonly socketId: SocketId;
      readonly value: ParameterValue;
    }
  | { readonly type: "node.socket-default-reset"; readonly id: NodeId; readonly socketId: SocketId }
  | { readonly type: "node.mute" | "node.collapse"; readonly id: NodeId; readonly value: boolean }
  | { readonly type: "node.parent"; readonly id: NodeId; readonly parentId: NodeId | null }
  | { readonly type: "link.add"; readonly link: GraphLink }
  | { readonly type: "link.remove"; readonly id: LinkId }
  | { readonly type: "link.mute"; readonly id: LinkId; readonly value: boolean }
  | { readonly type: "link.replace"; readonly removeId: LinkId; readonly link: GraphLink }
  | { readonly type: "undo" }
  | { readonly type: "redo" };
/** Atomic, deliberately non-recursive operations used by completed gestures. */
export type BatchCommand<C extends FxNodeCompositionData = FxNodeCompositionData> = Extract<
  Command<C>,
  {
    readonly type:
      | "node.move"
      | "node.resize"
      | "node.remove"
      | "node.mute"
      | "node.collapse"
      | "node.parent"
      | "link.remove"
      | "link.mute";
  }
>;
/** A forward-only command suitable for durable replay. */
export type FxNodeReplayCommand<C extends FxNodeCompositionData = FxNodeCompositionData> = Exclude<
  Command<C>,
  { readonly type: "undo" | "redo" }
>;
export interface FxNodeSaveData<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly kind: "fxnode.command-log";
  readonly schemaVersion: 2;
  readonly composition: C;
  readonly baseline: GraphLayoutV2;
  readonly commands: readonly FxNodeReplayCommand<C>[];
}
/** A decoded save whose embedded historical composition was compatibility-checked against C. */
export interface CompatibleFxNodeSaveData<C extends FxNodeCompositionData = FxNodeCompositionData>
  extends Omit<FxNodeSaveData<C>, "composition"> {
  readonly composition: FxNodeCompositionData;
}
export interface CommandRequest<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly commandId: CommandId;
  readonly expectedVersion: number;
  readonly source: "api" | "gesture";
  readonly command: Command<C>;
}
export interface CommandError {
  readonly code: string;
  readonly message: string;
  readonly path?: string;
}
