import type { Command, CompatibleFxNodeSaveData, FxNodeReplayCommand, FxNodeSaveData } from "../commands/types.js";
import { FXNODE_SAVE_DATA_LIMITS } from "../commands/save-data.js";
import type { FxNodeCompositionData } from "../composition/types.js";
import { admitStructuredData, cloneJson, deepFreeze, type StructuredDataMetrics } from "../core/json.js";
import type { GraphLayoutV2 } from "../core/types.js";

export interface JournalEntry<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly command: FxNodeReplayCommand<C>;
  readonly metrics: StructuredDataMetrics;
  readonly atomicCommands: number;
}
export interface WorkerJournal<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly baseline: GraphLayoutV2;
  readonly applied: readonly JournalEntry<C>[];
  readonly redo: readonly JournalEntry<C>[];
  readonly values: number;
  readonly stringCodeUnits: number;
  readonly atomicCommands: number;
  readonly depth: number;
}
const metrics = (command: FxNodeReplayCommand): JournalEntry | undefined => {
  const admitted = admitStructuredData(command, FXNODE_SAVE_DATA_LIMITS);
  if (!admitted.ok) return;
  return deepFreeze({
    command: admitted.value as FxNodeReplayCommand,
    metrics: admitted.metrics,
    atomicCommands: command.type === "batch" ? command.commands.length : 1,
  });
};
const fold = <C extends FxNodeCompositionData>(
  baseline: GraphLayoutV2,
  applied: readonly JournalEntry<C>[],
  redo: readonly JournalEntry<C>[],
): WorkerJournal<C> =>
  deepFreeze({
    baseline: cloneJson(baseline),
    applied: [...applied],
    redo: [...redo],
    values: applied.reduce((n, x) => n + x.metrics.values, 0),
    stringCodeUnits: applied.reduce((n, x) => n + x.metrics.stringCodeUnits, 0),
    atomicCommands: applied.reduce((n, x) => n + x.atomicCommands, 0),
    depth: applied.reduce((n, x) => Math.max(n, x.metrics.depth), 0),
  });
export const checkpointJournal = <C extends FxNodeCompositionData>(baseline: GraphLayoutV2): WorkerJournal<C> =>
  fold(baseline, [], []);
export const importJournal = <C extends FxNodeCompositionData>(
  data: CompatibleFxNodeSaveData<C> | FxNodeSaveData<C>,
): WorkerJournal<C> =>
  fold(
    data.baseline,
    data.commands.map((command) => metrics(command) as JournalEntry<C>),
    [],
  );
export const journalSaveData = <C extends FxNodeCompositionData>(
  journal: WorkerJournal<C>,
  composition: C,
): FxNodeSaveData<C> =>
  cloneJson({
    kind: "fxnode.command-log",
    schemaVersion: 2,
    composition,
    baseline: journal.baseline,
    commands: journal.applied.map((entry) => entry.command),
  } as FxNodeSaveData<C>);
export function advanceJournal<C extends FxNodeCompositionData>(
  journal: WorkerJournal<C>,
  command: Command<C>,
  candidateBaseline: GraphLayoutV2,
  strictReplayValid: (baseline: GraphLayoutV2, commands: readonly FxNodeReplayCommand<C>[]) => boolean,
): WorkerJournal<C> {
  let applied = journal.applied,
    redo = journal.redo;
  if (command.type === "undo") {
    const entry = applied.at(-1);
    if (!entry) return checkpointJournal(candidateBaseline);
    applied = applied.slice(0, -1);
    redo = [...redo, entry];
  } else if (command.type === "redo") {
    const entry = redo.at(-1);
    if (!entry) return checkpointJournal(candidateBaseline);
    applied = [...applied, entry];
    redo = redo.slice(0, -1);
  } else {
    const entry = metrics(command);
    if (!entry) return checkpointJournal(candidateBaseline);
    applied = [...applied, entry as JournalEntry<C>];
    redo = [];
  }
  const next = fold(journal.baseline, applied, redo),
    over =
      next.applied.length > FXNODE_SAVE_DATA_LIMITS.maxCommands ||
      next.atomicCommands > FXNODE_SAVE_DATA_LIMITS.maxAtomicCommands ||
      next.values > FXNODE_SAVE_DATA_LIMITS.maxValues ||
      next.stringCodeUnits > FXNODE_SAVE_DATA_LIMITS.maxStringCodeUnits ||
      next.depth > FXNODE_SAVE_DATA_LIMITS.maxDepth;
  return over ||
    !strictReplayValid(
      next.baseline,
      next.applied.map((entry) => entry.command),
    )
    ? checkpointJournal(candidateBaseline)
    : next;
}
