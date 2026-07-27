import test from "node:test";
import assert from "node:assert/strict";
import {
  adaptFxNodeSnapshot,
  AuthoringGraphError,
  getSourceMap,
  mapAuthoringDiagnostic,
} from "../static/render-graph/adapter.js";
import { RendererError } from "../static/renderer-client.js";
import {
  semanticCatalog,
  nodeDefinitions,
  GRAPH_ID,
  CATALOG_VERSION,
  socketTypes,
} from "../static/render-graph/catalog.js";
import { culling } from "../static/render-graph/presets.js";
import { AuthoringController } from "../static/render-graph/authoring-controller.js";
function fixture() {
  const nodes = culling.nodes.map((n) => {
    const d = semanticCatalog[n.executor.key];
    const definition = nodeDefinitions[n.executor.key];
    return {
      id: n.id,
      typeId: n.executor.key,
      typeVersion: 1,
      known: true,
      muted: n.state !== "enabled",
      position: { x: 10, y: 20 },
      size: { x: 200, y: 120 },
      label: n.id,
      collapsed: false,
      extensions: {},
      parameters: Object.fromEntries(
        Object.entries(definition.parameters).map(([key, schema]) => [
          key,
          {
            kind: schema.type,
            value: structuredClone(n.parameters[key] ?? schema.default.value),
          },
        ]),
      ),
      sockets: [
        ...Object.entries(d.inputs).map(([key, x]) => ({
          key,
          id: `${n.id}:${key}`,
          direction: "input",
          dataType: x.authoringType ?? x.accepted.types[0],
          label: key,
          accepts: socketTypes[x.authoringType ?? x.accepted.types[0]].acceptsFrom,
          visible: true,
          maxIncomingLinks: 1,
        })),
        ...Object.entries(d.outputs).map(([key]) => ({
          key,
          id: `${n.id}:${key}`,
          direction: "output",
          dataType: definition.sockets[key].type,
          label: key,
          accepts: [],
          visible: true,
          maxIncomingLinks: 0,
        })),
      ],
    };
  });
  const links = [];
  for (const n of culling.nodes)
    for (const [socket, from] of Object.entries(n.inputs))
      links.push({
        id: `l_${from.node}_${from.socket}_${n.id}_${socket}`,
        fromNodeId: from.node,
        fromSocketId: `${from.node}:${from.socket}`,
        toNodeId: n.id,
        toSocketId: `${n.id}:${socket}`,
        muted: false,
        extensions: {},
      });
  return {
    graphId: GRAPH_ID,
    catalogVersion: CATALOG_VERSION,
    nodes,
    links,
    metadata: { layout: "ignored" },
    version: 1,
  };
}
test("catalog exhaustively mirrors all current contracts", () => {
  assert.deepEqual(
    Object.keys(semanticCatalog).sort(),
    [
      "surface_target",
      "texture_spec",
      "scene_table",
      "local_aabb_buffer",
      "camera_frustum",
      "visibility_flags",
      "frustum_cull",
      "mesh_query",
      "depth_stencil_config",
      "legacy_forward",
      "fullscreen_copy",
      "tone_map",
      "bloom_extract",
      "bloom_blur",
      "bloom_composite",
      "luminance_edge",
      "present",
    ].sort(),
  );
  for (const c of Object.values(semanticCatalog)) {
    assert.ok(c.execution);
    assert.ok(c.inputs);
    assert.ok(c.outputs);
    assert.ok(c.parameters);
  }
});
test("adapter deterministically emits the canonical schema, permits repeated types, omits muted links and maps sources", () => {
  const x = fixture(),
    a = adaptFxNodeSnapshot(x, 7);
  x.nodes.reverse();
  x.links.reverse();
  assert.deepEqual(adaptFxNodeSnapshot(x, 7), a);
  assert.equal(a.schemaVersion, 2);
  assert.equal(a.graphId, GRAPH_ID);
  assert.equal(
    a.nodes.filter((n) => n.executor.key === "texture_spec").length,
    2,
  );
  assert.ok(
    Object.values(getSourceMap(a)).some((source) => source.input === "source"),
  );
  x.links.find((l) => l.id === "l_forward_color_copy_source").muted = true;
  assert.equal(
    adaptFxNodeSnapshot(x, 8).nodes.find((n) => n.id === "copy").inputs.source,
    undefined,
  );
});
test("adapter rejects hostile shape, IDs, duplicates, catalog, sockets and type mismatches", () => {
  const reject = (fn, code) => {
    const x = fixture();
    fn(x);
    assert.throws(
      () => adaptFxNodeSnapshot(x),
      (e) => e instanceof AuthoringGraphError && e.code === code,
    );
  };
  reject((x) => (x.graphId = "bad"), "AUTHORING_CATALOG");
  reject((x) => (x.nodes[0].id = "bad id"), "AUTHORING_ID");
  reject((x) => (x.nodes[1].id = x.nodes[0].id), "AUTHORING_ID_DUPLICATE");
  reject((x) => (x.nodes[0].typeId = "wat"), "AUTHORING_NODE_TYPE");
  reject((x) => (x.nodes[0].sockets = []), "AUTHORING_SOCKET_SET");
  reject((x) => (x.links[0].toSocketId = "missing:x"), "AUTHORING_LINK");
  reject((x) => {
    const link = x.links.find((l) => l.toSocketId === "copy:source");
    link.fromNodeId = "scene";
    link.fromSocketId = "scene:scene";
  }, "AUTHORING_LINK_TYPE");
});
test("adapter counts only active incoming links and reports socket overflow", () => {
  const x = fixture();
  const active = x.links.find((link) => link.toSocketId === "copy:source");
  x.links.push({ ...structuredClone(active), id: "muted_duplicate", muted: true });
  assert.doesNotThrow(() => adaptFxNodeSnapshot(x));
  x.links.push({ ...structuredClone(active), id: "active_overflow" });
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_LINK_INCOMING" && error.details.socketId === "copy:source",
  );
});
test("source map covers Rust fields, nested values, every input and is deeply frozen", () => {
  const snapshot = fixture();
  snapshot.links.find((link) => link.toSocketId === "copy:source").muted = true;
  const ir = adaptFxNodeSnapshot(snapshot, 9), map = getSourceMap(ir);
  for (const path of ["schemaVersion", "graphId", "revision", "nodes", "nodes[0].id", "nodes[0].state", "nodes[0].executor.key", "nodes[0].executor.version", "nodes[0].parameters", "nodes[0].inputs"])
    assert.ok(map[path], path);
  for (const [index, node] of ir.nodes.entries())
    for (const input of Object.keys(semanticCatalog[node.executor.key].inputs))
      assert.ok(map[`nodes[${index}].inputs.${input}`]);
  assert.ok(Object.keys(map).some((path) => /parameters\..+\[|parameters\..+\..+/.test(path)));
  const socket = Object.values(map).find((source) => source.kind === "socket");
  const unconnected = Object.values(map).find((source) => source.unconnected === true);
  const link = Object.values(map).find((source) => source.kind === "link");
  assert.ok(socket?.socketId && unconnected?.socketId);
  assert.equal(ir.nodes.find((node) => node.id === "copy").inputs.source, undefined);
  for (const field of ["linkId", "fromNodeId", "fromSocketId", "toNodeId", "toSocketId", "muted"])
    assert.ok(Object.hasOwn(link, field), field);
  assert.ok(Object.isFrozen(map) && Object.isFrozen(link));
});
test("diagnostic mapper creates a frozen RendererError DTO with fallbacks and prefix matching", () => {
  const ir = adaptFxNodeSnapshot(fixture());
  const original = new RendererError("GRAPH_INPUT", { message: "bad", field: "nodes[0].executor.key.more", nested: { x: 1 } });
  const mapped = mapAuthoringDiagnostic(ir, original);
  assert.notStrictEqual(mapped, original);
  assert.equal(mapped.code, original.code);
  assert.equal(mapped.source.kind, "node");
  assert.equal(mapped.diagnostic, undefined);
  assert.ok(Object.isFrozen(mapped) && Object.isFrozen(mapped.details.nested));
  assert.equal(Object.isFrozen(original), false);
  original.details.nested.x = 2;
  assert.equal(mapped.details.nested.x, 1);
  const unmatchedOriginal = new RendererError("GRAPH_INPUT", {
    message: "unmapped",
    path: "resources[0]",
  });
  const unmatched = mapAuthoringDiagnostic(ir, unmatchedOriginal);
  assert.notStrictEqual(unmatched, unmatchedOriginal);
  assert.equal(unmatched.source, undefined);
  assert.ok(Object.isFrozen(unmatched) && Object.isFrozen(unmatched.details));
  assert.equal(Object.isFrozen(unmatchedOriginal), false);
});
test("controller keeps last-good through failures and only drops after successful switch", async () => {
  let fail = false;
  const calls = [];
  const renderer = {
    compileGraph: async (ir) => ({
      compiledId: [ir.revision, 1],
      revision: ir.revision,
    }),
    switchCompiledGraph: async (id) => {
      calls.push(["switch", id]);
      if (fail) throw Error("switch");
    },
    dropCompiledGraph: async (id) => calls.push(["drop", id]),
  };
  const c = new AuthoringController({
    renderer,
    adapt: (_, revision) => ({ revision }),
  });
  c.markDirty({});
  await c.apply();
  fail = true;
  c.markDirty({});
  await assert.rejects(c.apply());
  assert.deepEqual(calls, [
    ["switch", [1, 1]],
    ["switch", [2, 1]],
  ]);
  fail = false;
  await c.apply();
  assert.deepEqual(calls.at(-1), ["drop", [1, 1]]);
  await c.destroy();
});
test("controller shares in-flight apply", async () => {
  let release;
  const gate = new Promise((r) => (release = r));
  const c = new AuthoringController({
    adapt: (_, revision) => ({ revision }),
    renderer: {
      compileGraph: async (ir) => {
        await gate;
        return { compiledId: [ir.revision, 1], revision: ir.revision };
      },
      switchCompiledGraph: async () => {},
      dropCompiledGraph: async () => {},
    },
  });
  c.markDirty({});
  const a = c.apply();
  assert.strictEqual(c.apply(), a);
  release();
  await a;
});
test("controller retains mapped diagnostic while apply rejects the original and subscriptions agree", async () => {
  const original = new RendererError("GRAPH_BAD", { path: "nodes[0].id", message: "bad" });
  const states = [];
  const c = new AuthoringController({
    adapt: (snapshot, revision) => adaptFxNodeSnapshot(snapshot, revision),
    renderer: { compileGraph: async () => { throw original; }, switchCompiledGraph: async () => {}, dropCompiledGraph: async () => {} },
  });
  c.subscribe((state) => states.push(state));
  c.markDirty(fixture());
  await assert.rejects(c.apply(), (error) => error === original);
  assert.notStrictEqual(states.at(-1).error, original);
  let subscribed;
  c.subscribe((state) => { subscribed = state; })();
  assert.strictEqual(subscribed.error, states.at(-1).error);
  await c.destroy();
});
test("apply after destroy does not compile and destroy returns one strict promise", async () => {
  let compiles = 0;
  const c = new AuthoringController({ adapt: () => ({}), renderer: { compileGraph: async () => { compiles++; }, dropCompiledGraph: async () => {}, switchCompiledGraph: async () => {} } });
  c.markDirty({});
  const first = c.destroy();
  assert.strictEqual(c.destroy(), first);
  assert.strictEqual(await c.apply(), null);
  await first;
  assert.equal(compiles, 0);
});
