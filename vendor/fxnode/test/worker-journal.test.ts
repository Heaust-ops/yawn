import assert from "node:assert/strict";
import test from "node:test";
import type { FxNodeSaveData } from "@lib/commands/types.js";
import { createFxNodeHeadless } from "@lib/headless-runtime.js";
import { nodeId } from "@lib/core/types.js";
import { advanceJournal, checkpointJournal, importJournal, journalSaveData } from "@lib/worker/journal.js";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";

const composition = {
    ...APPLICATION_COMPILED.source,
    id: "journal",
    version: 1,
    nodes: { value: APPLICATION_COMPILED.source.nodes["fxnode.shader.value"]! },
  },
  runtime = createFxNodeHeadless(composition),
  baseline = runtime.save(runtime.emptyDocument()),
  authority = composition,
  valid = () => true;
const add = { type: "node.add", nodeId: nodeId("n"), nodeType: "value", position: { x: 0, y: 0 } } as const,
  move = { type: "node.move", id: nodeId("n"), position: { x: 1, y: 2 } } as const;

test("worker journal folds forward undo redo and clears redo on a new forward", () => {
  const empty = checkpointJournal(baseline),
    one = advanceJournal(empty, add, baseline, valid),
    two = advanceJournal(one, move, baseline, valid);
  assert.deepEqual(journalSaveData(two, authority).commands, [add, move]);
  const undone = advanceJournal(two, { type: "undo" }, baseline, valid);
  assert.deepEqual(journalSaveData(undone, authority).commands, [add]);
  assert.equal(undone.redo.length, 1);
  const redone = advanceJournal(undone, { type: "redo" }, baseline, valid);
  assert.deepEqual(journalSaveData(redone, authority).commands, [add, move]);
  const replaced = advanceJournal(undone, { ...move, position: { x: 3, y: 4 } }, baseline, valid);
  assert.equal(replaced.redo.length, 0);
  assert.deepEqual(journalSaveData(replaced, authority).commands.at(-1), { ...move, position: { x: 3, y: 4 } });
  assert.equal(Object.isFrozen(replaced), true);
  assert.equal(Object.isFrozen(journalSaveData(replaced, authority)), true);
});

test("worker journal checkpoints on strict replay failure and imported journals append", () => {
  const one = advanceJournal(checkpointJournal(baseline), add, baseline, valid),
    saved = journalSaveData(one, authority),
    imported = importJournal(saved as unknown as FxNodeSaveData<typeof composition>),
    appended = advanceJournal(imported, move, baseline, valid);
  assert.deepEqual(journalSaveData(appended, authority).commands, [add, move]);
  const checkpointed = advanceJournal(one, move, baseline, () => false);
  assert.equal(checkpointed.applied.length, 0);
  assert.equal(JSON.stringify(checkpointed.baseline), JSON.stringify(baseline));
  const undone = advanceJournal(checkpointed, { type: "undo" }, { ...baseline, metadata: { after: "undo" } }, valid);
  assert.deepEqual(undone.baseline.metadata, { after: "undo" });
  assert.equal(undone.applied.length, 0);
  const oversized = advanceJournal(
    one,
    { type: "node.label", id: nodeId("n"), label: "x".repeat(1_048_577) },
    { ...baseline, metadata: { after: "large command" } },
    valid,
  );
  assert.deepEqual(oversized.baseline.metadata, { after: "large command" });
  assert.equal(oversized.applied.length, 0);
});

test("strict callback receives only baseline and applied commands", () => {
  let received: readonly unknown[] = [];
  const next = advanceJournal(checkpointJournal(baseline), add, baseline, (candidate, commands) => {
    received = [candidate, commands];
    return true;
  });
  assert.equal(received[0], next.baseline);
  assert.deepEqual(received[1], [add]);
  assert.equal("composition" in (received[0] as object), false);
});
