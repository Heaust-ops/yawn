/**
 * Browser client and static composition API for fxnode.
 *
 * The root client owns graph/composition authority and its worker until {@link FxNode.destroy}.
 * Canvases and host interaction belong to independently attached {@link FxNodeView} handles.
 * @module fxnode
 */
export * from "./browser/client.js";
export type {
  FxNodeModifiers,
  FxNodeInput,
  FxNodeViewport,
  FxNodeCamera,
  FxNodeHostSnapshot,
  AddNodeParams,
  FxNodeActionOptions,
  FxNodeSelectionSnapshot,
  FxNodeAddNodeMenuRequest,
  FxNodeResourceAuthorization,
  FxNodeImageResourceDescriptor,
  FxNodeResourceOpenRequest,
  FxNodeResourceData,
  FxNodeHostRequest,
} from "./browser/host-types.js";
export { FXNODE_VIEW_LIMITS, fxNodeDevicePixels } from "./browser/view-limits.js";
export * from "./core/types.js";
export type { Command, BatchCommand, FxNodeReplayCommand, FxNodeSaveData } from "./commands/types.js";
export type { Mutation } from "./engine/mutations.js";
export type { MutationEnvelope, SnapshotEnvelope } from "./composition/bound-engine.js";
export * from "./composition/index.js";
