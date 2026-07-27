import assert from "node:assert/strict";
import test from "node:test";
import { createFxNodeHeadless } from "@lib/headless-runtime.js";
import { commandId, nodeId, type GraphState } from "@lib/core/types.js";
import { rebindBoundEngineAuthority } from "@lib/composition/bound-engine.js";
import type { FxNodeCompositionData } from "@lib/composition/types.js";
import { canonicalJsonEqual } from "@lib/core/json.js";
import { admitStructuredData } from "@lib/core/json.js";
import { PERSISTENCE_LIMITS } from "@lib/composition/bound-document.js";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";

const composition = {
  ...APPLICATION_COMPILED.source,
  id: "runtime-test",
  version: 91,
  nodes: {
    value: APPLICATION_COMPILED.source.nodes["fxnode.shader.value"]!,
    frame: APPLICATION_COMPILED.source.nodes["fxnode.common.frame"]!,
  },
};
const runtime = createFxNodeHeadless(composition);
const request = (version: number, command: Parameters<typeof runtime.transition>[1]["command"]) => ({
  commandId: commandId(`c${version}`),
  expectedVersion: version,
  source: "api" as const,
  command,
});

test("bound runtime materializes, edits, resets, undoes, redoes, saves and loads", () => {
  assert.equal(runtime.emptyDocument().catalogVersion, 91);
  const materialized = runtime.materializeNode("direct", "value");
  assert.equal(materialized.typeId, "value");
  let state = runtime.createEngine(runtime.emptyDocument());
  let result = runtime.transition(
    state,
    request(0, { type: "node.add", nodeId: nodeId("n"), nodeType: "value", position: { x: 1, y: 2 } }),
  );
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  state = result.state;
  const key = Object.keys(state.document.nodes.n!.parameters)[0]!;
  result = runtime.transition(
    state,
    request(1, { type: "node.parameter", id: nodeId("n"), key, value: { kind: "number", value: 4 } }),
  );
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  state = result.state;
  result = runtime.transition(state, request(2, { type: "node.parameter-reset", id: nodeId("n"), key }));
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  state = result.state;
  result = runtime.transition(state, request(3, { type: "undo" }));
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  result = runtime.transition(result.state, request(4, { type: "redo" }));
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  const saved = runtime.save(result.state.document),
    decoded = runtime.decodeGraphDocument(saved);
  assert.equal(decoded.ok, true);
  const loaded = runtime.load(result.state, saved);
  assert.equal(loaded.ok, true);
  assert.equal(
    runtime.serializeGraphDocument(result.state.document),
    runtime.serializeGraphDocument(decoded.ok ? decoded.value : result.state.document),
  );
});

test("bound runtime rejects unknown add and invalid initial engine", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  const bad = { type: "node.add", nodeId: nodeId("x"), nodeType: "missing", position: { x: 0, y: 0 } } as never;
  const result = runtime.transition(state, request(0, bad));
  assert.equal(result.status, "rejected");
  if (result.status === "rejected") assert.equal(result.error.code, "node.type-unknown");
  assert.throws(() =>
    runtime.createEngine({
      ...runtime.emptyDocument(),
      nodes: { x: { ...runtime.materializeNode("x", "value"), size: { x: 0, y: 1 } } },
    }),
  );
  assert.throws(() => runtime.createEngine({ ...runtime.emptyDocument(), catalogVersion: 999 }));
  assert.throws(() =>
    runtime.createEngine({
      ...runtime.emptyDocument(),
      links: {
        wrong: {
          id: "actual",
          fromNodeId: "a",
          fromSocketId: "a:out",
          toNodeId: "b",
          toSocketId: "b:in",
          muted: false,
          extensions: {},
        },
      },
    } as never),
  );
});

test("graph state decoding is exact for current composition and ignores only observational version", () => {
  const document = {
      ...runtime.emptyDocument("state-source"),
      nodes: { n: runtime.materializeNode("n", "value", { x: 4, y: 5 }) },
    },
    state = runtime.createEngine(document),
    snapshot = runtime.getState(state),
    input = structuredClone(snapshot),
    nested = input.nodes[0]!;
  const decoded = runtime.decodeGraphState(input);
  assert.equal(decoded.ok, true);
  assert.deepEqual(input, snapshot);
  assert.equal(Object.isFrozen(input), false);
  assert.equal(Object.isFrozen(nested), false);
  if (!decoded.ok) return;
  assert.equal(decoded.value.nodes.n?.known, true);
  assert.ok(Object.isFrozen(decoded.value));
  const spoofed = runtime.decodeGraphState({ ...snapshot, version: Number.MAX_SAFE_INTEGER });
  assert.equal(spoofed.ok, true);
  const missingKnown = { ...snapshot, nodes: snapshot.nodes.map(({ known: _, ...node }) => node) },
    wrongKnown = { ...snapshot, nodes: snapshot.nodes.map((node) => ({ ...node, known: false })) },
    foreign = { ...snapshot, catalogVersion: 999 };
  for (const value of [missingKnown, wrongKnown, foreign]) {
    const result = runtime.decodeGraphState(value);
    assert.equal(result.ok, false);
    if (!result.ok) assert.equal(result.issues[0]?.code, "state.inexact");
  }
  const malformed = runtime.decodeGraphState({ ...snapshot, extra: true });
  assert.equal(malformed.ok, false);
  if (!malformed.ok) assert.equal(malformed.issues[0]?.code, "state.shape");
  const failed = structuredClone(wrongKnown),
    failedNested = failed.nodes[0]!;
  runtime.decodeGraphState(failed);
  assert.equal(Object.isFrozen(failed), false);
  assert.equal(Object.isFrozen(failedNested), false);
  assert.deepEqual(failed, wrongKnown);
  const cycle: any = {};
  cycle.self = cycle;
  const sparse: any = structuredClone(snapshot);
  sparse.nodes.length = 2;
  const nonfinite: any = structuredClone(snapshot);
  nonfinite.nodes[0].position.x = Infinity;
  const overLimit: any = { ...snapshot, nodes: Array(100_001).fill(null) };
  for (const value of [cycle, sparse, nonfinite, overLimit])
    assert.doesNotThrow(() => assert.equal(runtime.decodeGraphState(value).ok, false));
});

test("maximum admitted durable layout round-trips through getState and state decoding", () => {
  const base = runtime.save({
      ...runtime.emptyDocument("state-limit"),
      nodes: { n: runtime.materializeNode("n", "value") },
    }),
    metrics = admitStructuredData(base, PERSISTENCE_LIMITS);
  assert.equal(metrics.ok, true);
  if (!metrics.ok) return;
  const paddingLength = PERSISTENCE_LIMITS.maxValues - metrics.metrics.values - 1,
    layout = { ...base, metadata: { padding: Array(paddingLength).fill(null) } } as typeof base,
    admitted = admitStructuredData(layout, PERSISTENCE_LIMITS);
  assert.equal(admitted.ok, true);
  if (!admitted.ok) return;
  assert.equal(admitted.metrics.values, PERSISTENCE_LIMITS.maxValues);
  const document = runtime.decodeGraphDocument(layout);
  assert.equal(document.ok, true);
  if (!document.ok) return;
  const state = runtime.getState(runtime.createEngine(document.value)),
    decoded = runtime.decodeGraphState(state);
  assert.equal(decoded.ok, true);
  if (decoded.ok) assert.equal((decoded.value.metadata.padding as readonly unknown[]).length, paddingLength);
});

test("state replacement commits once and is one-step undoable and redoable", () => {
  const targetDocument = {
      ...runtime.emptyDocument("replacement"),
      nodes: { n: runtime.materializeNode("n", "value", { x: 7, y: 8 }) },
    },
    target = runtime.getState(runtime.createEngine(targetDocument));
  let state = runtime.createEngine(runtime.emptyDocument("before"), 2);
  const replaced = runtime.replaceState(state, { commandId: commandId("replace"), expectedVersion: 0, target });
  assert.equal(replaced.status, "committed");
  if (replaced.status !== "committed") return;
  assert.equal(replaced.state.version, 1);
  assert.equal(replaced.state.document.graphId, "replacement");
  assert.equal(replaced.state.undo.length, 1);
  assert.equal(replaced.mutationEnvelope.cause, "api");
  assert.deepEqual(
    replaced.mutationEnvelope.mutations.map((m) => m.kind),
    ["document.replaced"],
  );
  state = replaced.state;
  const undo = runtime.transition(state, request(1, { type: "undo" }));
  assert.equal(undo.status, "committed");
  if (undo.status !== "committed") return;
  assert.equal(undo.state.version, 2);
  assert.equal(undo.state.document.graphId, "before");
  assert.equal(undo.state.redo.length, 1);
  const redo = runtime.transition(undo.state, request(2, { type: "redo" }));
  assert.equal(redo.status, "committed");
  if (redo.status !== "committed") return;
  assert.equal(redo.state.version, 3);
  assert.equal(redo.state.document.graphId, "replacement");
  const noop = runtime.replaceState(redo.state, {
    commandId: commandId("noop"),
    expectedVersion: 3,
    target: runtime.getState(redo.state),
  });
  assert.equal(noop.status, "noop");
  assert.equal(noop.state, redo.state);
  const withoutHistory = runtime.createEngine(runtime.emptyDocument("zero"), 0),
    zero = runtime.replaceState(withoutHistory, { commandId: commandId("zero"), expectedVersion: 0, target });
  assert.equal(zero.status, "committed");
  if (zero.status === "committed") assert.equal(zero.state.undo.length, 0);
});

test("state replacement checks staleness before inspecting hostile input and noops before overflow", () => {
  const state = runtime.createEngine(runtime.emptyDocument());
  let inspections = 0;
  const hostile = Object.defineProperty({}, "graphId", {
    enumerable: true,
    get() {
      inspections++;
      throw new Error("must not inspect");
    },
  }) as GraphState<typeof composition>;
  const stale = runtime.replaceState(state, {
    commandId: commandId("stale-state"),
    expectedVersion: 1,
    target: hostile,
  });
  assert.equal(stale.status, "rejected");
  assert.equal(stale.state, state);
  assert.equal(inspections, 0);
  if (stale.status === "rejected") assert.equal(stale.error.code, "version.stale");
  const current = runtime.replaceState(state, {
    commandId: commandId("hostile-state"),
    expectedVersion: 0,
    target: hostile,
  });
  assert.equal(current.status, "rejected");
  assert.equal(inspections, 0);
  if (current.status === "rejected") assert.equal(current.error.code, "data.inspect");
  const maximum = { ...state, version: Number.MAX_SAFE_INTEGER } as typeof state,
    target = runtime.getState(state),
    noop = runtime.replaceState(maximum, {
      commandId: commandId("maximum-noop"),
      expectedVersion: Number.MAX_SAFE_INTEGER,
      target,
    });
  assert.equal(noop.status, "noop");
  assert.equal(noop.state, maximum);
  const changed = { ...target, graphId: "changed" as typeof target.graphId },
    overflow = runtime.replaceState(maximum, {
      commandId: commandId("maximum-change"),
      expectedVersion: Number.MAX_SAFE_INTEGER,
      target: changed,
    });
  assert.equal(overflow.status, "rejected");
  assert.equal(overflow.state, maximum);
  if (overflow.status === "rejected") assert.equal(overflow.error.code, "version.overflow");
});

test("state replacement preserves bounded prior history, clears redo, and ignores spoofed target version", () => {
  const target = runtime.getState(
    runtime.createEngine({
      ...runtime.emptyDocument("target"),
      nodes: { target: runtime.materializeNode("target", "value") },
    }),
  );
  let state = runtime.createEngine(runtime.emptyDocument("history"), 2),
    first = runtime.transition(
      state,
      request(0, { type: "node.add", nodeId: nodeId("a"), nodeType: "value", position: { x: 0, y: 0 } }),
    );
  assert.equal(first.status, "committed");
  if (first.status !== "committed") return;
  state = first.state;
  const second = runtime.transition(
    state,
    request(1, { type: "node.add", nodeId: nodeId("b"), nodeType: "value", position: { x: 0, y: 0 } }),
  );
  assert.equal(second.status, "committed");
  if (second.status !== "committed") return;
  const undone = runtime.transition(second.state, request(2, { type: "undo" }));
  assert.equal(undone.status, "committed");
  if (undone.status !== "committed") return;
  assert.equal(undone.state.redo.length, 1);
  const replaced = runtime.replaceState(undone.state, {
    commandId: commandId("history-replace"),
    expectedVersion: 3,
    target: { ...target, version: 999 } as GraphState<typeof composition>,
  });
  assert.equal(replaced.status, "committed");
  if (replaced.status !== "committed") return;
  assert.equal(replaced.state.version, 4);
  assert.equal(replaced.state.redo.length, 0);
  assert.equal(replaced.state.undo.length, 2);
  assert.deepEqual(
    replaced.state.undo.at(-1)?.forward.map((m) => m.kind),
    ["document.replaced"],
  );
  assert.deepEqual(
    replaced.state.undo.at(-1)?.inverse.map((m) => m.kind),
    ["document.replaced"],
  );
  const undoIdentity = replaced.state.undo,
    redoIdentity = replaced.state.redo,
    entryIdentity = replaced.state.undo[0],
    noop = runtime.replaceState(replaced.state, {
      commandId: commandId("history-noop"),
      expectedVersion: 4,
      target: runtime.getState(replaced.state),
    });
  assert.equal(noop.status, "noop");
  assert.equal(noop.state.undo, undoIdentity);
  assert.equal(noop.state.redo, redoIdentity);
  assert.equal(noop.state.undo[0], entryIdentity);
  const limited = runtime.createEngine(runtime.emptyDocument("limited"), 1),
    added = runtime.transition(
      limited,
      request(0, { type: "node.add", nodeId: nodeId("old"), nodeType: "value", position: { x: 0, y: 0 } }),
    );
  assert.equal(added.status, "committed");
  if (added.status !== "committed") return;
  const one = runtime.replaceState(added.state, {
    commandId: commandId("limited-replace"),
    expectedVersion: 1,
    target,
  });
  assert.equal(one.status, "committed");
  if (one.status === "committed") {
    assert.equal(one.state.undo.length, 1);
    assert.deepEqual(
      one.state.undo[0]?.forward.map((m) => m.kind),
      ["document.replaced"],
    );
  }
});

test("composition runtimes isolate overlapping IDs; unknown versions roundtrip read-only", () => {
  const other = createFxNodeHeadless({
    ...composition,
    id: "other",
    version: 92,
    nodes: { ...composition.nodes, value: { ...composition.nodes.value, title: "Other" } },
  });
  assert.equal(other.materializeNode("a", "value").label, "Other");
  const raw = runtime.save({ ...runtime.emptyDocument(), nodes: { a: runtime.materializeNode("a", "value") } });
  const unknownVersion = { ...raw, nodes: raw.nodes.map((n) => ({ ...n, typeVersion: n.typeVersion + 1 })) };
  const decoded = runtime.decodeGraphDocument(unknownVersion);
  assert.equal(decoded.ok, true);
  if (!decoded.ok) return;
  assert.equal(decoded.value.nodes.a!.known, false);
  assert.equal(runtime.parseGraphDocument(runtime.serializeGraphDocument(decoded.value)).ok, true);
  let state = runtime.createEngine(decoded.value);
  const key = Object.keys(state.document.nodes.a!.parameters)[0]!;
  const edit = runtime.transition(state, request(0, { type: "node.parameter-reset", id: nodeId("a"), key }));
  assert.equal(edit.status, "rejected");
  const malformed = { ...raw, nodes: raw.nodes.map((n) => ({ ...n, parameters: {} })) };
  assert.equal(runtime.decodeGraphDocument(malformed).ok, false);
});

test("foreign catalog versions normalize and future node payloads remain opaque", () => {
  const raw = runtime.save({ ...runtime.emptyDocument(), nodes: { a: runtime.materializeNode("a", "value") } });
  const foreign = { ...raw, catalogVersion: 999 };
  const normalized = runtime.decodeGraphDocument(foreign);
  assert.equal(normalized.ok, true);
  if (!normalized.ok) return;
  assert.equal(normalized.value.catalogVersion, 91);
  assert.equal(runtime.save(normalized.value).catalogVersion, 91);
  const loaded = runtime.load(runtime.createEngine(runtime.emptyDocument()), foreign);
  assert.equal(loaded.ok, true);
  if (loaded.ok) assert.equal(loaded.snapshotEnvelope.snapshot.catalogVersion, 91);
  const future = {
    ...raw,
    nodes: raw.nodes.map((n) => ({
      ...n,
      typeId: "future.node",
      typeVersion: 12,
      parameters: { future: { history: [] } },
      extensions: { session: { selection: [1] } },
      futureField: { hovered: true },
      sockets: [
        {
          ...n.sockets[0]!,
          dataType: "future-socket",
          accepts: ["future-source"],
          defaultValue: null,
          metadata: { runtimeVersion: 7 },
        },
      ],
    })),
  };
  const decoded = runtime.decodeGraphDocument(future);
  assert.equal(decoded.ok, true);
  if (!decoded.ok) return;
  assert.equal(decoded.value.nodes.a!.known, false);
  assert.equal(JSON.stringify(runtime.save(decoded.value)), JSON.stringify(future));
});

test("authority rebind preserves graph version for presentation changes and resets history", () => {
  const source: FxNodeCompositionData = composition,
    candidateSource: FxNodeCompositionData = { ...source, theme: { ...source.theme, background: "#123456" } },
    current = createFxNodeHeadless(source),
    candidate = createFxNodeHeadless(candidateSource);
  let state = current.createEngine(current.emptyDocument(), 7),
    result = current.transition(state, {
      commandId: commandId("add"),
      expectedVersion: 0,
      source: "api",
      command: { type: "node.add", nodeId: nodeId("n"), nodeType: "value", position: { x: 0, y: 0 } },
    });
  assert.equal(result.status, "committed");
  if (result.status !== "committed") return;
  state = result.state;
  assert.equal(state.undo.length, 1);
  const rebound = rebindBoundEngineAuthority(state, current, candidate, { commandId: commandId("rebind") });
  assert.equal(rebound.ok, true);
  if (!rebound.ok) return;
  assert.equal(rebound.graphChanged, false);
  assert.equal(rebound.state.version, state.version);
  assert.equal(rebound.state.historyLimit, 7);
  assert.equal(rebound.state.undo.length, 0);
  assert.equal(rebound.state.redo.length, 0);
  assert.equal(rebound.state.document.nodes.n!.known, true);
  assert.equal(canonicalJsonEqual({ b: 1, a: 2 }, { a: 2, b: 1 }), true);
  const left = Object.create(null) as Record<string, unknown>,
    right = Object.create(null) as Record<string, unknown>;
  left.__proto__ = { value: 1 };
  right.__proto__ = { value: 2 };
  assert.equal(canonicalJsonEqual(left, right), false);
});

test("authority rebind promotes opaque nodes and explicitly removed definitions remain opaque", () => {
  const full: FxNodeCompositionData = composition,
    withoutFrame: FxNodeCompositionData = { ...full, nodes: { value: full.nodes.value! } },
    old = createFxNodeHeadless(withoutFrame),
    candidate = createFxNodeHeadless(full);
  const authored = candidate.save({
      ...candidate.emptyDocument(),
      nodes: { f: candidate.materializeNode("f", "frame") },
    }),
    opaque = old.decodeGraphDocument(authored);
  assert.equal(opaque.ok, true);
  if (!opaque.ok) return;
  assert.equal(opaque.value.nodes.f!.known, false);
  const promoted = rebindBoundEngineAuthority(old.createEngine(opaque.value), old, candidate, {
    commandId: commandId("promote"),
  });
  assert.equal(promoted.ok, true);
  if (!promoted.ok || !promoted.graphChanged) return;
  assert.equal(promoted.state.version, 1);
  assert.equal(promoted.state.document.nodes.f!.known, true);
  assert.equal(promoted.mutationEnvelope.cause, "composition");
  assert.equal(promoted.mutationEnvelope.mutations[0]?.kind, "document.replaced");
  assert.deepEqual(old.save(opaque.value), candidate.save(promoted.state.document));

  const current = createFxNodeHeadless(full),
    removed = createFxNodeHeadless(withoutFrame),
    known = current.createEngine({ ...current.emptyDocument(), nodes: { f: current.materializeNode("f", "frame") } });
  const rejected = rebindBoundEngineAuthority(known, current, removed, { commandId: commandId("reject") });
  assert.equal(rejected.ok, false);
  if (rejected.ok) return;
  assert.equal(rejected.state, known);
  assert.equal(rejected.issues[0]?.code, "composition.node-demotion");
  const accepted = rebindBoundEngineAuthority(known, current, removed, {
    commandId: commandId("remove"),
    removedNodeTypes: new Set(["frame"]),
  });
  assert.equal(accepted.ok, true);
  if (!accepted.ok) return;
  assert.equal(accepted.graphChanged, true);
  assert.equal(accepted.state.document.nodes.f!.known, false);
  assert.equal(accepted.state.document.nodes.f!.id, "f");
});

test("graph-changing authority rebind rejects version overflow atomically", () => {
  const full: FxNodeCompositionData = composition,
    removedSource: FxNodeCompositionData = { ...full, nodes: { value: full.nodes.value! } },
    current = createFxNodeHeadless(full),
    removed = createFxNodeHeadless(removedSource);
  const normal = current.createEngine({
      ...current.emptyDocument(),
      nodes: { f: current.materializeNode("f", "frame") },
    }),
    state = { ...normal, version: Number.MAX_SAFE_INTEGER, undo: [{ forward: [], inverse: [] }] } as typeof normal;
  const result = rebindBoundEngineAuthority(state, current, removed, {
    commandId: commandId("overflow"),
    removedNodeTypes: new Set(["frame"]),
  });
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.equal(result.state, state);
  assert.equal(result.issues[0]?.code, "version.overflow");
  assert.equal(result.state.undo.length, 1);
});
