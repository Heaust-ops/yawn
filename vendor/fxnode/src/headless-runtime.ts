import { compileFxNodeComposition } from "./composition/compile.js";
import { bindDocument } from "./composition/bound-document.js";
import { bindEngine } from "./composition/bound-engine.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData, NodeTypeId } from "./composition/types.js";
import type { ReferenceCheck } from "./composition/references.js";
import type { CommandId, GraphDocument, GraphLayoutV2, GraphNode, GraphSnapshot, Socket, Vec2 } from "./core/types.js";
import type { CommandRequest, FxNodeReplayCommand } from "./commands/types.js";
import type { DecodeResult, ValidationIssue } from "./composition/bound-document.js";
import type {
  EngineState,
  LoadResult,
  ReplayResult,
  StateReplacementRequest,
  TransitionResult,
} from "./composition/bound-engine.js";

/** Explicit immutable document and engine operations bound to one composition authority. */
export interface FxNodeHeadless<C extends FxNodeCompositionData> {
  /** Creates an empty immutable document for this composition. */
  emptyDocument(id?: string): GraphDocument<C>;
  /** Materializes a known node using its definition's defaults. */
  materializeNode(id: string, typeId: NodeTypeId<C>, position?: Vec2, parentId?: string): GraphNode<C>;
  /** Validates an immutable document without changing it. */
  validateDocument(document: GraphDocument<C>): readonly ValidationIssue[];
  /** Decodes and validates durable graph data under this composition. */
  decodeGraphDocument(source: unknown): DecodeResult<C>;
  /** Decodes a graph-state shaped value into a validated document. */
  decodeGraphState(source: unknown): DecodeResult<C>;
  /** Parses JSON text and decodes it as a graph document. */
  parseGraphDocument(text: string): DecodeResult<C>;
  /** Converts a validated document to its stable, durable layout form. */
  save(document: GraphDocument<C>): GraphLayoutV2;
  /** Serializes the stable durable layout as canonical JSON. */
  serializeGraphDocument(document: GraphDocument<C>): string;
  socketsCompatible(
    from: Pick<Socket, "direction" | "dataType">,
    to: Pick<Socket, "direction" | "dataType" | "accepts">,
  ): boolean;
  /** Creates version-zero engine state; history retains 100 entries by default, or the supplied nonnegative limit. */
  createEngine(document: GraphDocument<C>, historyLimit?: number): EngineState<C>;
  /** Immutably applies a command. Committed results increment version; no-op and rejected results retain state/version. */
  transition(state: EngineState<C>, request: CommandRequest<C>): TransitionResult<C>;
  /** Atomically replaces graph state when `expectedVersion` matches; invalid or stale requests are rejected unchanged. */
  replaceState(state: EngineState<C>, request: StateReplacementRequest<C>): TransitionResult<C>;
  /** Atomically decodes compatible durable data and replaces state; failures preserve the original state. */
  load(state: EngineState<C>, value: unknown, expectedVersion?: number, id?: CommandId): LoadResult<C>;
  /** Validates compatibility and atomically replays save data; any invalid command preserves the original state. */
  replaySaveData(state: EngineState<C>, value: unknown, expectedVersion?: number, id?: CommandId): ReplayResult<C>;
  validateReplayJournal(
    baseline: GraphLayoutV2,
    commands: readonly FxNodeReplayCommand<C>[],
    historyLimit?: number,
  ): boolean;
  /** Returns a deeply immutable snapshot carrying the engine state's current version. */
  getState(state: EngineState<C>): GraphSnapshot<C>;
}
/** @internal Binds an already compiled authority without recompiling it. */
export function bindFxNodeHeadless<C extends FxNodeCompositionData>(
  compiled: CompiledFxNodeComposition<C>,
): FxNodeHeadless<C> {
  return Object.freeze({ ...bindDocument(compiled), ...bindEngine(compiled) });
}

/** Creates an isolated composition-bound document and engine runtime. The composition is compiled exactly once. */
export function createFxNodeHeadless<const C extends FxNodeCompositionData>(
  composition: C & ReferenceCheck<C>,
): FxNodeHeadless<C> {
  const compiled = compileFxNodeComposition(composition);
  return bindFxNodeHeadless(compiled);
}
