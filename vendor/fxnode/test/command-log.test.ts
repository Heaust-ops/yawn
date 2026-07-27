import assert from "node:assert/strict";
import test from "node:test";
import { validCommand, validFxNodeReplayCommand } from "@lib/commands/validate.js";
import { commandId, linkId, nodeId, socketId } from "@lib/core/types.js";
import { saveCompositionCompatibility } from "@lib/composition/save-compatibility.js";
import { createFxNodeHeadless } from "@lib/headless-runtime.js";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";
import type { FxNodeCompositionData } from "@lib/composition/index.js";

const composition = {
  ...APPLICATION_COMPILED.source,
  id: "command-log-test",
  version: 7,
  nodes: { value: APPLICATION_COMPILED.source.nodes["fxnode.shader.value"]! },
};
const runtime = createFxNodeHeadless(composition);
const save = (commands: readonly unknown[], baseline: unknown = runtime.save(runtime.emptyDocument())) => ({
  kind: "fxnode.command-log",
  schemaVersion: 2,
  composition,
  baseline,
  commands,
});

test("command validators are total and replay is forward-only", () => {
  const hostile = new Proxy(
    {},
    {
      ownKeys() {
        throw new Error("hostile");
      },
    },
  );
  assert.doesNotThrow(() => validCommand(hostile));
  assert.equal(validCommand(hostile), false);
  assert.equal(validCommand({ type: "undo" }), true);
  assert.equal(validFxNodeReplayCommand({ type: "undo" }), false);
  assert.equal(
    validFxNodeReplayCommand({ type: "batch", commands: [{ type: "node.move", id: "n", position: { x: 1, y: 2 } }] }),
    true,
  );
});

test("replay commits atomically once, preserves bounded staged history, and accepts empty logs", () => {
  const original = runtime.createEngine(runtime.emptyDocument(), 1),
    commands = [
      { type: "node.add", nodeId: nodeId("n"), nodeType: "value", position: { x: 0, y: 0 } } as const,
      { type: "node.move", id: nodeId("n"), position: { x: 2, y: 3 } } as const,
    ];
  const result = runtime.replaySaveData(original, save(commands), 0, commandId("restore"));
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  assert.equal(Object.isFrozen(result.saveData), true);
  assert.notEqual(result.saveData, save(commands));
  assert.equal(result.state.version, 1);
  assert.deepEqual(result.state.document.nodes.n!.position, { x: 2, y: 3 });
  assert.equal(result.state.undo.length, 1);
  assert.equal(result.state.redo.length, 0);
  assert.equal(result.mutationEnvelope.cause, "load");
  assert.equal(result.mutationEnvelope.mutations.length, 1);
  assert.equal(result.mutationEnvelope.mutations[0]?.kind, "document.replaced");
  const empty = runtime.replaySaveData(result.state, save([]));
  assert.equal(empty.ok, true);
  if (empty.ok) {
    assert.equal(empty.state.version, 2);
    assert.equal(empty.state.undo.length, 0);
  }
});

test("replay rejection, noop, composition mismatch, and stale source inspection are atomic", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  for (const data of [
    save([{ type: "node.remove", id: "missing" }]),
    save([{ type: "batch", commands: [] }]),
    { ...save([]), composition: { ...composition, id: "other" } },
  ]) {
    const result = runtime.replaySaveData(state, data);
    assert.equal(result.ok, false);
    if (!result.ok) assert.equal(result.state, state);
  }
  const noop = runtime.replaySaveData(state, save([{ type: "batch", commands: [] }]));
  assert.equal(noop.ok, false);
  if (!noop.ok) {
    assert.equal(noop.issues[0]?.code, "replay.noop");
    assert.equal(noop.issues[0]?.path, "/commands/0");
  }
  let inspected = false;
  const hostile = new Proxy(
    {},
    {
      ownKeys() {
        inspected = true;
        throw new Error("inspected");
      },
    },
  );
  const stale = runtime.replaySaveData(state, hostile, 1);
  assert.equal(stale.ok, false);
  assert.equal(inspected, false);
  if (!stale.ok) {
    assert.equal(stale.state, state);
    assert.equal(stale.issues[0]?.code, "version.stale");
  }
  const unchanged = runtime.replaySaveData(state, save([]));
  assert.equal(unchanged.ok, true);
  if (unchanged.ok) {
    assert.equal(unchanged.status, "noop");
    assert.equal(unchanged.state.version, state.version);
    assert.equal(unchanged.state.document, state.document);
  }
});

test("persisted commands reject malformed durable values and links atomically", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  const link = {
    id: "l",
    fromNodeId: "a",
    fromSocketId: "a:o",
    toNodeId: "b",
    toSocketId: "b:i",
    muted: false,
    extensions: {},
  };
  const malformed = [
    { type: "link.add", link: { ...link, muted: 0 } },
    { type: "link.add", link: { ...link, extensions: { bad: undefined } } },
    { type: "node.parameter", id: "n", key: "value", value: { kind: "number", value: "1" } },
    { type: "node.socket-default", id: "n", socketId: "n:o", value: { kind: "vector", value: [1, 2] } },
    { type: "node.add", nodeId: "n", nodeType: "not-bound", position: { x: 0, y: 0 } },
  ];
  for (const command of malformed) {
    const result = runtime.replaySaveData(state, save([command]));
    assert.equal(result.ok, false);
    if (!result.ok) {
      assert.equal(result.state, state);
      assert.match(result.issues[0]?.path ?? "", /^\/commands\/0/);
    }
  }
});

test("command and baseline admissions have independent budgets and require V2", () => {
  const state = runtime.createEngine(runtime.emptyDocument()),
    baseline = runtime.save(runtime.emptyDocument());
  const large = { ...baseline, metadata: { values: Array.from({ length: 100_001 }, () => 0) } };
  assert.equal(runtime.replaySaveData(state, save([], large)).ok, true);
  const commands = Array.from({ length: 1_001 }, () => ({ type: "node.remove", id: "n" }));
  const over = runtime.replaySaveData(state, save(commands, baseline));
  assert.equal(over.ok, false);
  if (!over.ok) assert.equal(over.state, state);
  const v1 = runtime.replaySaveData(state, save([], { ...baseline, schemaVersion: 1 }));
  assert.equal(v1.ok, false);
  if (!v1.ok) {
    assert.equal(v1.state, state);
    assert.equal(v1.issues[0]?.code, "baseline.schema");
  }
});

test("save schema v1 and future versions have distinct structured errors", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  for (const [schemaVersion, code] of [
    [1, "save.schema.unsupported"],
    [3, "save.schema.future"],
  ] as const) {
    const result = runtime.replaySaveData(state, { ...save([]), schemaVersion });
    assert.equal(result.ok, false);
    if (!result.ok) {
      assert.equal(result.issues[0]?.code, code);
      assert.equal(result.issues[0]?.path, "/schemaVersion");
      assert.equal(result.state, state);
    }
  }
});

test("save decode alone normalizes embedded schema-1 composition menus", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  const legacyComposition = {
    ...composition,
    schemaVersion: 1,
    menuGroups: { legacy: { title: "Legacy", order: 0 } },
    nodes: Object.fromEntries(
      Object.entries(composition.nodes).map(([id, node]) => [
        id,
        { ...node, menu: { kind: "entry", group: "legacy", order: 0, keywords: [] } },
      ]),
    ),
  };
  const result = runtime.replaySaveData(state, { ...save([]), composition: legacyComposition });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.saveData.composition.schemaVersion, 2);
  assert.equal("menuGroups" in result.saveData.composition, false);
  assert.equal("menu" in result.saveData.composition.nodes.value!, false);
  assert.equal(validateLegacyRaw(legacyComposition), false);

  for (const invalid of [
    { ...legacyComposition, menuGroups: undefined },
    { ...legacyComposition, nodes: { ...legacyComposition.nodes, value: composition.nodes.value } },
    {
      ...legacyComposition,
      nodes: {
        ...legacyComposition.nodes,
        value: { ...legacyComposition.nodes.value, menu: { kind: "entry", group: "missing", order: 0, keywords: [] } },
      },
    },
    { ...legacyComposition, unexpected: true },
  ]) {
    const rejected = runtime.replaySaveData(state, { ...save([]), composition: invalid });
    assert.equal(rejected.ok, false);
  }
});

const validateLegacyRaw = (value: unknown) => {
  try {
    createFxNodeHeadless(value as FxNodeCompositionData);
    return true;
  } catch {
    return false;
  }
};

test("composition compatibility accepts true supersets and rejects changed replay semantics", () => {
  const extra = APPLICATION_COMPILED.source.nodes["fxnode.shader.color"]!,
    presentation = {
      ...composition,
      version: 8,
      theme: { ...composition.theme, background: "#123456" as const },
      nodes: { ...composition.nodes, extra },
    };
  const reordered = {
    ...presentation,
    nodes: {
      ...presentation.nodes,
      value: { ...presentation.nodes.value!, ui: [...presentation.nodes.value!.ui].reverse() },
    },
  };
  const baseline = runtime.save(runtime.emptyDocument());
  assert.deepEqual(saveCompositionCompatibility(composition, reordered, baseline), []);
  const supersetRuntime = createFxNodeHeadless(reordered),
    accepted = supersetRuntime.replaySaveData(supersetRuntime.createEngine(supersetRuntime.emptyDocument()), save([]));
  assert.equal(accepted.ok, true);
  const changed = {
    ...presentation,
    nodes: {
      ...presentation.nodes,
      value: {
        ...presentation.nodes.value!,
        parameters: {
          value: { ...presentation.nodes.value!.parameters.value!, default: { kind: "number" as const, value: 42 } },
        },
      },
    },
  };
  const semantic = saveCompositionCompatibility(composition, changed as FxNodeCompositionData, baseline);
  assert.equal(semantic[0]?.code, "composition.incompatible");
  assert.equal(semantic[1]?.code, "composition.node-semantic");
  assert.equal(semantic[1]?.path, "/composition/nodes/value");
  const missing = { ...presentation, nodes: { extra } };
  const absent = saveCompositionCompatibility(composition, missing, baseline);
  assert.equal(absent[0]?.code, "composition.incompatible");
  assert.equal(absent[1]?.code, "composition.definition-missing");
  const forged = {
    ...save([{ type: "node.add", nodeId: "x", nodeType: "extra", position: { x: 0, y: 0 } }]),
    composition,
  };
  const rejected = supersetRuntime.replaySaveData(
    supersetRuntime.createEngine(supersetRuntime.emptyDocument()),
    forged,
  );
  assert.equal(rejected.ok, false);
  if (!rejected.ok) assert.equal(rejected.issues[0]?.code, "command.invalid");
});

test("compatibility guards opaque promotion and caps structured issues", () => {
  const extra = APPLICATION_COMPILED.source.nodes["fxnode.shader.color"]!,
    current = { ...composition, nodes: { ...composition.nodes, opaque: extra } };
  const opaque = { ...runtime.save(runtime.emptyDocument()), nodes: [{ id: "o", typeId: "opaque" }] as never };
  const promoted = saveCompositionCompatibility(composition, current, opaque);
  assert.equal(promoted[0]?.code, "composition.incompatible");
  assert.equal(promoted[1]?.code, "composition.opaque-promotion");
  assert.equal(promoted[1]?.path, "/baseline/nodes/0/typeId");
  const many = Object.fromEntries(
      Array.from({ length: 150 }, (_, i) => [`style-${i}`, { header: "#000000" }]),
    ) as unknown as typeof composition.nodeStyles,
    withMany = { ...composition, nodeStyles: many };
  const capped = saveCompositionCompatibility(
    withMany,
    { ...composition, nodeStyles: {} },
    runtime.save(runtime.emptyDocument()),
  );
  assert.equal(capped.length, 100);
});

test("trusted journal validation rejects executable commands outside the durable schema", () => {
  const full = createFxNodeHeadless(APPLICATION_COMPILED.source),
    from = full.materializeNode("from", "fxnode.shader.value"),
    to = full.materializeNode("to", "fxnode.shader.math"),
    baseline = full.save({ ...full.emptyDocument(), nodes: { from, to } }),
    link = {
      id: linkId("l"),
      fromNodeId: nodeId("from"),
      fromSocketId: socketId("from:value"),
      toNodeId: nodeId("to"),
      toSocketId: socketId("to:a"),
      muted: false,
      extensions: {},
    };
  assert.equal(full.validateReplayJournal(baseline, [{ type: "link.add", link }]), true);
  assert.equal(
    full.validateReplayJournal(baseline, [{ type: "link.add", link: { ...link, transient: "not durable" } as never }]),
    false,
  );
});

test("replay rejects a command whose resulting aggregate exceeds persistence closure", () => {
  const node = runtime.materializeNode("n", "value"),
    baseline = runtime.save({ ...runtime.emptyDocument(), nodes: { n: node } });
  const baselineText = "b".repeat(8_388_350),
    commandText = "c".repeat(1_048_554);
  const result = runtime.replaySaveData(
    runtime.createEngine(runtime.emptyDocument()),
    save([{ type: "node.label", id: "n", label: commandText }], { ...baseline, metadata: { text: baselineText } }),
  );
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.state.document.nodes.n, undefined);
});
