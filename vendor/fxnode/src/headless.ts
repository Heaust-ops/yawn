/**
 * Composition-bound graph and command execution without browser or worker resources.
 * @module fxnode/headless
 */
export * from "./core/types.js";
export type {
  Command,
  BatchCommand,
  FxNodeReplayCommand,
  FxNodeSaveData,
  CompatibleFxNodeSaveData,
  CommandRequest,
  CommandError,
} from "./commands/types.js";
export { FXNODE_SAVE_DATA_LIMITS } from "./commands/save-data.js";
export type { Mutation } from "./engine/mutations.js";
export type { DecodeResult, ValidationIssue } from "./composition/bound-document.js";
export type {
  EngineState,
  LoadResult,
  ReplayResult,
  MutationEnvelope,
  SnapshotEnvelope,
  StateReplacementRequest,
  TransitionResult,
} from "./composition/bound-engine.js";
export * from "./composition/index.js";
export type { FxNodeHeadless } from "./headless-runtime.js";
export { createFxNodeHeadless } from "./headless-runtime.js";
