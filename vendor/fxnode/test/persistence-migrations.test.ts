import assert from "node:assert/strict";
import test from "node:test";
import { PERSISTENCE_LIMITS } from "@lib/composition/bound-document.js";
import { createFxNodeHeadless } from "@lib/headless-runtime.js";

const numberValue = (value: number) => ({ kind: "number" as const, value });
const rampDefault = {
  kind: "json" as const,
  value: {
    colorMode: "rgb",
    interpolation: "linear",
    hueInterpolation: "near",
    stops: [
      { id: "a", position: 0, color: [0, 0, 0, 1] },
      { id: "b", position: 1, color: [1, 1, 1, 1] },
    ],
  },
};
const node = (migrations: readonly any[], version = 3) => ({
  version,
  title: "Arbitrary Migrating Thing",
  behavior: "standard" as const,
  style: "common",
  parameters: {
    gain: { type: "number" as const, default: numberValue(7) },
    bonus: { type: "number" as const, default: numberValue(11) },
    spectrum: { type: "json" as const, codec: "color-ramp/v1" as const, default: rampDefault },
  },
  sockets: {
    source: {
      title: "Source",
      direction: "output" as const,
      type: "float",
      maxIncomingLinks: 0,
      visible: true,
      value: null,
      showValue: false,
    },
    sink: {
      title: "Sink",
      direction: "input" as const,
      type: "float",
      maxIncomingLinks: 8,
      visible: true,
      value: null,
      showValue: false,
    },
  },
  ui: [
    { kind: "parameter" as const, parameter: "gain" },
    { kind: "parameter" as const, parameter: "bonus" },
    { kind: "widget" as const, widget: "color-ramp" as const, parameter: "spectrum" },
    { kind: "socket" as const, socket: "source" },
    { kind: "socket" as const, socket: "sink" },
  ],
  muteBypass: [],
  migrations,
});
const edges = [
  {
    fromVersion: 1,
    toVersion: 2,
    steps: [
      { kind: "rename-parameter", from: "strength", to: "gain" },
      { kind: "rename-socket", from: "send", to: "source" },
      { kind: "rename-socket", from: "receive", to: "sink" },
    ],
  },
  {
    fromVersion: 2,
    toVersion: 3,
    steps: [
      { kind: "materialize-missing", target: "parameter", key: "bonus" },
      { kind: "migrate-parameter", parameter: "spectrum", codec: "color-ramp/legacy-stops" },
    ],
  },
] as const;
const color = "#000000" as const;
const theme = Object.fromEntries(
  [
    "background",
    "grid",
    "frame",
    "frameHeader",
    "body",
    "control",
    "controlFill",
    "controlEditing",
    "textSelection",
    "outline",
    "text",
    "muted",
    "shadow",
    "nodeSelected",
    "nodeActive",
    "unknownHeader",
    "unknownSocket",
    "linkMuted",
    "knifeMuted",
    "emphasis",
    "focus",
    "editOutline",
    "resize",
    "muteOverlay",
    "boxSelectionFill",
    "checkerLight",
    "checkerDark",
    "widgetBorder",
    "rampBorder",
    "resourceBackground",
  ].map((key) => [key, color]),
);
const make = (migrations: readonly any[] = edges, version = 3) =>
  createFxNodeHeadless({
    schemaVersion: 2,
    id: "peculiar-migration-suite",
    version: 47,
    compatibility: { wildcardInputTypes: [] },
    theme,
    socketTypes: { float: { title: "Float", color, acceptsFrom: ["float"] } },
    nodeStyles: { common: { header: color } },
    resources: {},
    nodes: { "acme.odd-unit": node(migrations, version) },
  } as any);
const current = (runtime = make(), id = "n") =>
  runtime.save({ ...runtime.emptyDocument(), nodes: { [id]: runtime.materializeNode(id, "acme.odd-unit") } } as any);
const historical = (runtime = make(), id = "n") => {
  const raw = structuredClone(current(runtime, id));
  const n: any = raw.nodes[0];
  n.typeVersion = 1;
  n.parameters.strength = numberValue(23);
  delete n.parameters.gain;
  delete n.parameters.bonus;
  n.parameters.spectrum = [
    { position: 0, color: [0, 0, 0, 1] },
    { position: 1, color: [1, 0, 0, 1] },
  ];
  for (const s of n.sockets) {
    if (s.key === "source") {
      s.key = "send";
      s.id = `${id}:send`;
    } else {
      s.key = "receive";
      s.id = `${id}:receive`;
    }
  }
  return raw;
};
const decode = (runtime: ReturnType<typeof make>, raw: unknown) => {
  const result = runtime.decodeGraphDocument(raw);
  assert.equal(result.ok, true, result.ok ? undefined : JSON.stringify(result.issues));
  return result.ok ? result.value : runtime.emptyDocument();
};

test("custom parameter rename and missing default materialize", () => {
  const r = make(),
    d = decode(r, historical(r)),
    n: any = d.nodes.n;
  assert.deepEqual(n.parameters.gain, numberValue(23));
  assert.deepEqual(n.parameters.bonus, numberValue(11));
  assert.equal("strength" in n.parameters, false);
});
test("socket rename rewrites output/input endpoints and preserves link identity and extensions", () => {
  const r = make(),
    raw: any = historical(r);
  raw.nodes.push(structuredClone(raw.nodes[0]));
  raw.nodes[1].id = "m";
  for (const s of raw.nodes[1].sockets) s.id = s.id.replace(/^n:/, "m:");
  raw.links = [
    {
      id: "out",
      fromNodeId: "n",
      fromSocketId: "n:send",
      toNodeId: "m",
      toSocketId: "m:receive",
      muted: false,
      extensions: { custom: { x: 1 } },
    },
    {
      id: "in",
      fromNodeId: "m",
      fromSocketId: "m:send",
      toNodeId: "n",
      toSocketId: "n:receive",
      muted: true,
      extensions: { tag: "kept" },
    },
  ];
  const saved = r.save(decode(r, raw));
  assert.deepEqual(
    saved.links.map((x) => [x.id, x.fromSocketId, x.toSocketId, x.extensions]),
    [
      ["in", "m:source", "n:sink", { tag: "kept" }],
      ["out", "n:source", "m:sink", { custom: { x: 1 } }],
    ],
  );
});
test("graph state decoding never performs persistence migrations, materialization, rewrites, or reordering", () => {
  const r = make(),
    raw: any = historical(r);
  raw.nodes.push(structuredClone(raw.nodes[0]));
  raw.nodes[1].id = "m";
  for (const s of raw.nodes[1].sockets) s.id = s.id.replace(/^n:/, "m:");
  raw.links = [
    {
      id: "out",
      fromNodeId: "n",
      fromSocketId: "n:send",
      toNodeId: "m",
      toSocketId: "m:receive",
      muted: false,
      extensions: {},
    },
    {
      id: "in",
      fromNodeId: "m",
      fromSocketId: "m:send",
      toNodeId: "n",
      toSocketId: "n:receive",
      muted: false,
      extensions: {},
    },
  ];
  assert.equal(r.decodeGraphDocument(raw).ok, true);
  const state = {
      graphId: raw.graphId,
      catalogVersion: raw.catalogVersion,
      nodes: raw.nodes.map((node: any) => ({ ...node, known: true })),
      links: raw.links,
      metadata: raw.metadata,
    },
    migrating = r.decodeGraphState(state);
  assert.equal(migrating.ok, false);
  if (!migrating.ok) assert.equal(migrating.issues[0]?.code, "state.inexact");
  const currentState = r.getState(r.createEngine(decode(r, current(r)))),
    reordered = { ...currentState, nodes: [...currentState.nodes].reverse() };
  assert.equal(r.decodeGraphState(reordered).ok, currentState.nodes.length < 2);
  const promoted = { ...currentState, nodes: currentState.nodes.map((node: any) => ({ ...node, known: false })) },
    promotion = r.decodeGraphState(promoted);
  assert.equal(promotion.ok, false);
  if (!promotion.ok) assert.equal(promotion.issues[0]?.code, "state.inexact");
  const currentTwo = decode(r, { ...current(r), nodes: [...current(r, "a").nodes, ...current(r, "b").nodes] } as any),
    twoState = r.getState(r.createEngine(currentTwo)),
    reorderedNodes = { ...twoState, nodes: [...twoState.nodes].reverse() };
  assert.equal(r.decodeGraphState(reorderedNodes).ok, false);
  const currentLinks = decode(
      r,
      r.save(
        decode(r, {
          ...raw,
          nodes: raw.nodes.map((node: any) => ({
            ...node,
            typeVersion: 3,
            parameters: { gain: numberValue(7), bonus: numberValue(11), spectrum: structuredClone(rampDefault) },
            sockets: node.sockets.map((socket: any) => ({
              ...socket,
              key: socket.key === "send" ? "source" : "sink",
              id: socket.id.replace(/:(send|receive)$/, (_: string, key: string) =>
                key === "send" ? ":source" : ":sink",
              ),
            })),
          })),
          links: raw.links.map((link: any) => ({
            ...link,
            fromSocketId: link.fromSocketId.replace(":send", ":source"),
            toSocketId: link.toSocketId.replace(":receive", ":sink"),
          })),
        } as any),
      ),
    ),
    linkState = r.getState(r.createEngine(currentLinks)),
    reorderedLinks = { ...linkState, links: [...linkState.links].reverse() };
  assert.equal(r.decodeGraphState(reorderedLinks).ok, false);
});
test("legacy color-ramp codec works on arbitrary node and parameter", () => {
  const r = make(),
    n: any = decode(r, historical(r)).nodes.n;
  assert.equal(n.parameters.spectrum.kind, "json");
  assert.deepEqual(
    n.parameters.spectrum.value.stops.map((x: any) => x.id),
    ["stop-0", "stop-1"],
  );
});
test("direct and multi-edge migration paths execute", () => {
  const direct = make([
    { fromVersion: 2, toVersion: 3, steps: [{ kind: "materialize-missing", target: "parameter", key: "bonus" }] },
  ]);
  const raw: any = structuredClone(current(direct));
  raw.nodes[0].typeVersion = 2;
  delete raw.nodes[0].parameters.bonus;
  assert.equal((decode(direct, raw).nodes.n as any).known, true);
  assert.equal((decode(make(), historical()).nodes.n as any).typeVersion, 3);
});
test("catalogVersion mismatch is irrelevant and output is normalized", () => {
  const r = make(),
    raw = historical(r) as any;
  raw.catalogVersion = 999;
  const d = decode(r, raw);
  assert.equal(d.catalogVersion, 47);
  assert.equal(r.save(d).catalogVersion, 47);
});
test("missing route variants and unknown types preserve original save payload", () => {
  const cases: any[] = [];
  const noEdge: any = historical(make([]));
  cases.push([make([]), noEdge]);
  const gap: any = historical(make([edges[0]]));
  cases.push([make([edges[0]]), gap]);
  const future: any = structuredClone(current());
  future.nodes[0].typeVersion = 99;
  cases.push([make(), future]);
  const absent: any = structuredClone(current());
  absent.nodes[0].typeId = "vendor.absent";
  cases.push([make(), absent]);
  for (const [r, raw] of cases) {
    const d = decode(r, raw);
    assert.equal((Object.values(d.nodes)[0] as any).known, false);
    assert.deepEqual(r.save(d).nodes[0], raw.nodes[0]);
  }
});
test("failed migration steps and obsolete final fields remain original unknown", () => {
  const variants: any[] = [];
  const malformed: any = historical();
  malformed.nodes[0].parameters.spectrum = [{ bad: true }];
  variants.push(malformed);
  const missing: any = historical();
  delete missing.nodes[0].parameters.strength;
  variants.push(missing);
  const parameterCollision: any = historical();
  parameterCollision.nodes[0].parameters.gain = numberValue(4);
  variants.push(parameterCollision);
  const socketCollision: any = historical();
  socketCollision.nodes[0].sockets.push({
    ...structuredClone(socketCollision.nodes[0].sockets[0]),
    id: "n:source",
    key: "source",
  });
  variants.push(socketCollision);
  const obsolete: any = historical();
  obsolete.nodes[0].parameters.obsolete = numberValue(1);
  variants.push(obsolete);
  for (const raw of variants) {
    const r = make(),
      d = decode(r, raw);
    assert.equal((d.nodes.n as any).known, false);
    assert.deepEqual(r.save(d).nodes[0], raw.nodes[0]);
  }
});
test("malformed current-version payload hard fails catalog.invalid", () => {
  const r = make(),
    raw: any = structuredClone(current(r));
  delete raw.nodes[0].parameters.gain;
  const d = r.decodeGraphDocument(raw);
  assert.equal(d.ok, false);
  if (!d.ok) assert.ok(d.issues.some((x) => x.code === "catalog.invalid"));
});
test("current known nodes reject extra durable fields at every exactness level", () => {
  const variants = [
    (n: any) => {
      n.obsolete = true;
    },
    (n: any) => {
      n.parameters.gain.obsolete = true;
    },
    (n: any) => {
      n.sockets[0].obsolete = true;
    },
    (n: any) => {
      n.position.obsolete = true;
    },
  ];
  for (const mutate of variants) {
    const r = make(),
      raw: any = structuredClone(current(r));
    mutate(raw.nodes[0]);
    const decoded = r.decodeGraphDocument(raw);
    assert.equal(decoded.ok, false);
    if (!decoded.ok) assert.ok(decoded.issues.some((x) => x.code === "catalog.invalid"));
  }
});
test("historical extras abort migration and preserve the original unknown payload", () => {
  const variants = [
    (n: any) => {
      n.obsolete = true;
    },
    (n: any) => {
      n.parameters.strength.obsolete = true;
    },
    (n: any) => {
      n.sockets[0].obsolete = true;
    },
  ];
  for (const mutate of variants) {
    const r = make(),
      raw: any = historical(r);
    mutate(raw.nodes[0]);
    const decoded = decode(r, raw);
    assert.equal((decoded.nodes.n as any).known, false);
    assert.deepEqual(r.save(decoded).nodes[0], raw.nodes[0]);
  }
});
test("persisted known is rejected rather than silently stripped from opaque nodes", () => {
  const r = make(),
    raw: any = structuredClone(current(r));
  raw.nodes[0].typeId = "vendor.opaque";
  raw.nodes[0].known = false;
  const decoded = r.decodeGraphDocument(raw);
  assert.equal(decoded.ok, false);
  if (!decoded.ok) assert.ok(decoded.issues.some((x) => x.code === "decode.node"));
});
test("materialized defaults are independent clones", () => {
  const r = make(),
    a: any = r.materializeNode("a", "acme.odd-unit"),
    b: any = r.materializeNode("b", "acme.odd-unit");
  assert.notEqual(a.parameters.spectrum, b.parameters.spectrum);
  assert.notEqual(a.parameters.spectrum.value, b.parameters.spectrum.value);
});
test("materialized socket compatibility arrays are independently clone-safe", () => {
  const r = make(),
    a: any = r.materializeNode("a", "acme.odd-unit"),
    b: any = r.materializeNode("b", "acme.odd-unit"),
    raw = r.save({ ...r.emptyDocument(), nodes: { a, b } } as any);
  assert.notEqual(a.sockets[1].accepts, b.sockets[1].accepts);
  assert.equal(r.decodeGraphDocument(raw).ok, true);
});
test("link destination collision causes no partial node or link migration", () => {
  const r = make(),
    raw: any = historical(r);
  raw.links = [
    {
      id: "collision",
      fromNodeId: "n",
      fromSocketId: "n:send",
      toNodeId: "n",
      toSocketId: "n:sink",
      muted: false,
      extensions: { x: 1 },
    },
  ];
  const copy = structuredClone(raw),
    d = r.decodeGraphDocument(raw);
  assert.equal(d.ok, false);
  assert.deepEqual(raw, copy);
});
test("opaque unknown custom socket fields roundtrip", () => {
  const r = make(),
    raw: any = structuredClone(current(r));
  raw.nodes[0].typeId = "opaque.widget";
  raw.nodes[0].sockets[0] = {
    ...raw.nodes[0].sockets[0],
    dataType: "quark",
    accepts: ["boson"],
    defaultValue: null,
    metadata: { private: { v: 1 } },
    extensions: { socketExtra: true },
  };
  assert.deepEqual(r.save(decode(r, raw)), raw);
});
test("caller input is unchanged after successful and failed decode", () => {
  const r = make();
  for (const raw of [
    historical(r),
    (() => {
      const x: any = structuredClone(current(r));
      delete x.nodes[0].parameters.gain;
      return x;
    })(),
  ]) {
    const copy = structuredClone(raw);
    r.decodeGraphDocument(raw);
    assert.deepEqual(raw, copy);
  }
});
test("decode-save-decode stabilizes", () => {
  const r = make(),
    first = r.save(decode(r, historical(r))),
    second = r.save(decode(r, first));
  assert.deepEqual(second, first);
});
test("getter, cycle, sparse and nonfinite inputs error without throwing or executing getter", () => {
  const r = make();
  let calls = 0;
  const getter: any = {
    get schemaVersion() {
      calls++;
      throw Error("no");
    },
  };
  const cycle: any = {};
  cycle.self = cycle;
  const sparse: any = structuredClone(current(r));
  sparse.nodes.length = 2;
  const hugeSparse = new Array(4_000_000_000);
  const nonfinite: any = structuredClone(current(r));
  nonfinite.nodes[0].position.x = Infinity;
  for (const value of [getter, cycle, sparse, hugeSparse, nonfinite])
    assert.doesNotThrow(() => assert.equal(r.decodeGraphDocument(value).ok, false));
  const huge = r.decodeGraphDocument(hugeSparse);
  assert.equal(huge.ok, false);
  if (!huge.ok) assert.equal(huge.issues[0]?.code, "limit.values");
  assert.equal(calls, 0);
});
test("node and link collection limits reject over-limit and admit at-limit", () => {
  const r = make(),
    base: any = { schemaVersion: 2, graphId: "limits", catalogVersion: 1, nodes: [], links: [], metadata: {} };
  base.nodes = Array(PERSISTENCE_LIMITS.maxNodes + 1).fill(null);
  let d = r.decodeGraphDocument(base);
  assert.equal(d.ok, false);
  if (!d.ok) assert.equal(d.issues[0]?.path, "/nodes");
  base.nodes = [];
  base.links = Array(PERSISTENCE_LIMITS.maxLinks + 1).fill(null);
  d = r.decodeGraphDocument(base);
  assert.equal(d.ok, false);
  if (!d.ok) assert.equal(d.issues[0]?.path, "/links");
  base.links = [];
  assert.equal(r.decodeGraphDocument(base).ok, true);
});
test("bound runtime failed load returns the identical state object with version and history", () => {
  const r = make();
  let state: any = r.createEngine(r.emptyDocument());
  const request: any = {
    commandId: "x",
    expectedVersion: 0,
    source: "api",
    command: { type: "node.add", nodeId: "n", nodeType: "acme.odd-unit", position: { x: 0, y: 0 } },
  };
  const added = r.transition(state, request);
  assert.equal(added.status, "committed");
  if (added.status === "committed") state = added.state;
  const bad: any = structuredClone(current(r));
  delete bad.nodes[0].parameters.gain;
  const result = r.load(state, bad);
  assert.equal(result.ok, false);
  assert.equal(result.state, state);
  assert.equal(result.state.version, 1);
  assert.equal(result.state.undo, state.undo);
});
