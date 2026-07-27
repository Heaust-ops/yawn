import test from "node:test";
import assert from "node:assert/strict";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";
import { commandId, linkId, nodeId, socketId } from "@lib/core/types.js";
import { reduceMutations } from "@lib/engine/reducer.js";
import { minimumNodeSize } from "@lib/layout/node-dimensions.js";
import { validCommand } from "@lib/commands/validate.js";
import { planSelectionMute, planSelectionRemoval } from "@lib/worker/selection-actions.js";
const {
  createEngine,
  decodeGraphDocument,
  emptyDocument,
  materializeNode,
  save,
  serializeGraphDocument,
  getState,
  transition,
} = APPLICATION_HEADLESS;
const BUILTIN_DESCRIPTORS = Object.freeze([...APPLICATION_COMPILED.nodes.values()]);
const CATALOG_NODE_IDS = [...APPLICATION_COMPILED.nodes.keys()];
import type { EngineState, Command, CommandRequest } from "@lib/headless.js";

type ApplicationEngineState = EngineState<typeof APPLICATION_COMPILED.source>;

let sequence = 0;
function run(state: ApplicationEngineState, command: Command) {
  const request: CommandRequest = {
    commandId: commandId(`command-${++sequence}`),
    expectedVersion: state.version,
    source: "api",
    command,
  };
  return transition(state, request);
}
function committed(state: ApplicationEngineState, command: Command): ApplicationEngineState {
  const result = run(state, command);
  assert.equal(result.status, "committed");
  if (result.status !== "committed") throw new Error("expected commit");
  return result.state;
}

test("catalog has exact, frozen coverage and every built-in materializes generic initial state", () => {
  const expectedTypeIds = [
    "fxnode.common.frame",
    "fxnode.common.reroute",
    "fxnode.common.group-input",
    "fxnode.common.group-output",
    "fxnode.shader.value",
    "fxnode.shader.color",
    "fxnode.shader.math",
    "fxnode.shader.vector-math",
    "fxnode.shader.mix",
    "fxnode.shader.color-ramp",
    "fxnode.shader.texture-coordinate",
    "fxnode.shader.noise-texture",
    "fxnode.shader.image-texture",
    "fxnode.shader.principled-bsdf",
    "fxnode.shader.material-output",
    "fxnode.geometry.position",
    "fxnode.geometry.mesh-cube",
    "fxnode.geometry.set-position",
    "fxnode.geometry.transform-geometry",
    "fxnode.geometry.join-geometry",
    "fxnode.compositor.image",
    "fxnode.compositor.color-balance",
  ] as const;
  assert.deepEqual(
    BUILTIN_DESCRIPTORS.map((item) => item.typeId),
    expectedTypeIds,
  );
  assert.deepEqual(CATALOG_NODE_IDS, expectedTypeIds);
  assert.equal(BUILTIN_DESCRIPTORS.length, 22);
  assert.ok(BUILTIN_DESCRIPTORS.every(Object.isFrozen));
  assert.ok(Object.isFrozen(BUILTIN_DESCRIPTORS));
  assert.ok(Object.isFrozen(BUILTIN_DESCRIPTORS[0]!.sockets));
  assert.ok(Object.isFrozen(BUILTIN_DESCRIPTORS[4]!.parameters));
  for (const [index, typeId] of expectedTypeIds.entries()) {
    const node = materializeNode(`catalog-${index}`, typeId, { x: index, y: -index });
    assert.equal(node.typeId, typeId);
    assert.deepEqual(node.position, { x: index, y: -index });
    assert.equal(
      node.size.y,
      typeId === "fxnode.common.reroute" ? 10 : Math.max(100, minimumNodeSize(BUILTIN_DESCRIPTORS[index]!, node).y),
    );
    assert.equal(node.known, true);
    assert.equal(node.muted, false);
    assert.equal(node.collapsed, false);
    assert.equal(node.parentId, undefined);
    assert.deepEqual(Object.keys(node.extensions), []);
    assert.ok(Object.isFrozen(node));
    assert.ok(Object.isFrozen(node.parameters));
    assert.ok(Object.isFrozen(node.sockets));
  }
});

test("commit envelopes carry versions, command id, cause and explicit null", () => {
  const state = createEngine(emptyDocument());
  const id = commandId("add-1");
  const result = transition(state, {
    commandId: id,
    expectedVersion: 0,
    source: "gesture",
    command: { type: "node.add", nodeId: nodeId("n"), nodeType: "fxnode.shader.value", position: { x: 0, y: 0 } },
  });
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  assert.deepEqual(
    {
      base: result.mutationEnvelope.baseVersion,
      version: result.mutationEnvelope.version,
      cause: result.mutationEnvelope.cause,
    },
    { base: 0, version: 1, cause: "gesture" },
  );
  assert.equal(result.mutationEnvelope.commandId, id);
  assert.equal(result.mutationEnvelope.mutations[0]?.before, null);
  assert.equal(result.snapshotEnvelope.version, 1);
});

test("stale, duplicate and missing commands reject with exact state identity", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("n"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  for (const command of [
    { type: "node.add", nodeId: nodeId("n"), nodeType: "fxnode.shader.value", position: { x: 0, y: 0 } },
    { type: "node.remove", id: nodeId("missing") },
  ] as const) {
    const result = run(state, command);
    assert.equal(result.status, "rejected");
    assert.equal(result.state, state);
  }
  const stale = transition(state, {
    commandId: commandId("stale"),
    expectedVersion: 0,
    source: "api",
    command: { type: "undo" },
  });
  assert.equal(stale.status, "rejected");
  assert.equal(stale.state, state);
});

test("same-value editing is a no-op and untouched records are shared", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("a"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("b"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  const noop = run(state, { type: "node.move", id: nodeId("a"), position: { x: 0, y: 0 } });
  assert.equal(noop.status, "noop");
  assert.equal(noop.state, state);
  const before = state.document.nodes.b;
  state = committed(state, { type: "node.move", id: nodeId("a"), position: { x: 5, y: 6 } });
  assert.equal(state.document.nodes.b, before);
  assert.ok(Object.isFrozen(state.document.nodes.a?.parameters));
});

test("undo and redo restore remove cascades and frame children", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("frame"),
    nodeType: "fxnode.common.frame",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("child"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
    parentId: nodeId("frame"),
  });
  const beforeRemoval = state,
    removal = run(state, { type: "node.remove", id: nodeId("frame") });
  assert.equal(removal.status, "committed");
  if (removal.status !== "committed") return;
  state = removal.state;
  assert.equal(state.document.nodes.child?.parentId, undefined);
  assert.equal(Object.hasOwn(state.document.nodes.child!, "parentId"), false);
  assert.deepEqual(reduceMutations(beforeRemoval.document, removal.mutationEnvelope.mutations), state.document);
  const unparented = save(state.document),
    savedChild = unparented.nodes.find((node) => node.id === "child");
  assert.equal(Object.hasOwn(savedChild!, "parentId"), false);
  assert.equal(decodeGraphDocument(unparented).ok, true);
  state = committed(state, { type: "undo" });
  assert.equal(state.document.nodes.child?.parentId, "frame");
  state = committed(state, { type: "redo" });
  assert.equal(state.document.nodes.frame, undefined);
});

test("history limits 0, 1 and 2 bound undo and new edits clear redo", () => {
  for (const limit of [0, 1, 2]) {
    let state = createEngine(emptyDocument(), limit);
    state = committed(state, {
      type: "node.add",
      nodeId: nodeId(`n${limit}`),
      nodeType: "fxnode.shader.value",
      position: { x: 0, y: 0 },
    });
    assert.equal(state.undo.length, limit === 0 ? 0 : 1);
  }
  assert.throws(() => createEngine(emptyDocument(), 1.5), RangeError);
});

test("frame cycles reject atomically", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("a"),
    nodeType: "fxnode.common.frame",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("b"),
    nodeType: "fxnode.common.frame",
    position: { x: 0, y: 0 },
    parentId: nodeId("a"),
  });
  const result = run(state, { type: "node.parent", id: nodeId("a"), parentId: nodeId("b") });
  assert.equal(result.status, "rejected");
  assert.equal(result.state, state);
});

test("save, decode and save is canonical; snapshot arrays sort by id", () => {
  let state = createEngine(emptyDocument("canonical"));
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("z"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("a"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  const layout = save(state.document);
  assert.deepEqual(
    layout.nodes.map((item) => item.id),
    ["a", "z"],
  );
  const decoded = decodeGraphDocument(structuredClone(layout));
  assert.equal(decoded.ok, true);
  if (decoded.ok) assert.equal(serializeGraphDocument(decoded.value), serializeGraphDocument(state.document));
  assert.deepEqual(
    getState(state).nodes.map((item) => item.id),
    ["a", "z"],
  );
  assert.equal("known" in layout.nodes[0]!, false);
});

test("transient fields are rejected even through extension escape hatches", () => {
  const raw = { ...save(emptyDocument()), extensions: { selection: [] } };
  assert.equal(decodeGraphDocument(raw).ok, false);
  const nested = { ...save(emptyDocument()), metadata: { extension: { history: [] } } };
  assert.equal(decodeGraphDocument(nested).ok, false);
});

test("bound persistence normalizes v1 and rejects future document schemas", () => {
  const v1 = { schemaVersion: 1, graphId: "old", catalogVersion: 1, nodes: [], links: [], metadata: {} };
  const migrated = decodeGraphDocument(v1);
  assert.equal(migrated.ok, true);
  if (migrated.ok) assert.equal(migrated.value.catalogVersion, emptyDocument().catalogVersion);
  const future = decodeGraphDocument({ ...v1, schemaVersion: 3 });
  assert.equal(future.ok, false);
  if (!future.ok) assert.equal(future.issues.at(-1)?.code, "schema.future");
});

test("parameter reset resolves descriptor default and is one-step undoable", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("v"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.parameter",
    id: nodeId("v"),
    key: "value",
    value: { kind: "number", value: 9 },
  });
  state = committed(state, { type: "node.parameter-reset", id: nodeId("v"), key: "value" });
  assert.deepEqual(state.document.nodes.v?.parameters.value, { kind: "number", value: 0 });
  state = committed(state, { type: "undo" });
  assert.deepEqual(state.document.nodes.v?.parameters.value, { kind: "number", value: 9 });
});

test("batch is atomic, increments once, and undoes as one entry", () => {
  let state = createEngine(emptyDocument());
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("a"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  state = committed(state, {
    type: "node.add",
    nodeId: nodeId("b"),
    nodeType: "fxnode.shader.value",
    position: { x: 0, y: 0 },
  });
  const before = state,
    result = run(state, {
      type: "batch",
      commands: [
        { type: "node.move", id: nodeId("a"), position: { x: 3, y: 4 } },
        { type: "node.move", id: nodeId("b"), position: { x: 5, y: 6 } },
      ],
    });
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  assert.equal(result.state.version, before.version + 1);
  assert.equal(result.state.undo.length, before.undo.length + 1);
  state = committed(result.state, { type: "undo" });
  assert.deepEqual(state.document.nodes.a?.position, { x: 0, y: 0 });
  assert.deepEqual(state.document.nodes.b?.position, { x: 0, y: 0 });
  const rejected = run(state, {
    type: "batch",
    commands: [
      { type: "node.move", id: nodeId("a"), position: { x: 9, y: 9 } },
      { type: "node.remove", id: nodeId("missing") },
    ],
  });
  assert.equal(rejected.status, "rejected");
  assert.equal(rejected.state, state);
});

test("selection planners are complete, deterministic, and enforce the shared atomic limit", () => {
  const a = materializeNode("a", "fxnode.shader.value", { x: 0, y: 0 })!,
    b = materializeNode("b", "fxnode.shader.value", { x: 1, y: 1 })!;
  const incident = {
      id: linkId("incident"),
      fromNodeId: nodeId("a"),
      fromSocketId: socketId("a:out"),
      toNodeId: nodeId("b"),
      toSocketId: socketId("b:in"),
      muted: false,
      extensions: {},
    },
    standalone = { ...incident, id: linkId("standalone"), fromNodeId: nodeId("b") };
  const document = { ...emptyDocument(), nodes: { a, b }, links: { incident, standalone } };
  const removal = planSelectionRemoval(
    document,
    new Set([nodeId("a")]),
    new Set([linkId("incident"), linkId("standalone")]),
  )!;
  assert.deepEqual(removal, {
    type: "batch",
    commands: [
      { type: "link.remove", id: linkId("standalone") },
      { type: "node.remove", id: nodeId("a") },
    ],
  });
  const mixed = { ...document, nodes: { a: { ...a, muted: true }, b } };
  assert.deepEqual(
    planSelectionMute(mixed, new Set([nodeId("b"), nodeId("a")]), () => true),
    { type: "batch", commands: [{ type: "node.mute", id: nodeId("b"), value: true }] },
  );
  const nodes = Object.fromEntries(
    Array.from({ length: 257 }, (_, index) => {
      const id = `n-${String(index).padStart(3, "0")}`;
      return [id, materializeNode(id, "fxnode.shader.value", { x: index, y: 0 })!];
    }),
  );
  const overflow = planSelectionRemoval(
    { ...emptyDocument(), nodes },
    new Set(Object.keys(nodes).map(nodeId)),
    new Set(),
  )!;
  assert.equal(overflow.type, "batch");
  if (overflow.type !== "batch") return;
  assert.equal(overflow.commands.length, 257);
  assert.equal(validCommand(overflow), false);
  const overflowState = createEngine({ ...emptyDocument(), nodes }),
    rejected = run(overflowState, overflow);
  assert.equal(rejected.status, "rejected");
  assert.equal(rejected.state, overflowState);
  assert.equal(overflowState.undo.length, 0);
  assert.deepEqual(
    planSelectionMute(mixed, new Set([nodeId("b"), nodeId("a")]), (id) => id === nodeId("a"), false),
    { type: "batch", commands: [{ type: "node.mute", id: nodeId("a"), value: false }] },
  );
});
