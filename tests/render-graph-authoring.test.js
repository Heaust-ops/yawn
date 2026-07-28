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
  descriptors,
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
      typeVersion: d.version,
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
        ...Object.entries(d.inputs).map(([key, x]) => {
          const socket = definition.sockets[key];
          return {
            key,
            id: `${n.id}:${key}`,
            direction: "input",
            dataType: x.authoringType ?? x.accepted.types[0],
            label: socket.title,
            accepts:
              socketTypes[x.authoringType ?? x.accepted.types[0]].acceptsFrom,
            ...(socket.value
              ? { defaultValue: structuredClone(socket.value.default) }
              : {}),
            visible: socket.visible,
            maxIncomingLinks: socket.maxIncomingLinks,
          };
        }),
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
  for (const [key, semantic] of Object.entries(semanticCatalog)) {
    assert.ok(Object.hasOwn(semantic, "version"));
    assert.equal(semantic.version, key === "frame_out" ? 3 : 1);
    assert.equal(nodeDefinitions[key].version, semantic.version);
    assert.equal(descriptors[key].version, semantic.version);
  }
  assert.deepEqual(
    Object.keys(semanticCatalog),
    [
      "mesh",
      "texture",
      "frustum_cull",
      "mesh_query",
      "pipeline_registry",
      "pipeline",
      "fullscreen_copy",
      "color_balance",
      "exposure_contrast",
      "saturation",
      "channel_mixer",
      "bloom_extract",
      "bloom_blur",
      "bloom_composite",
      "luminance_edge",
      "frame_out",
    ],
  );
  for (const c of Object.values(semanticCatalog)) {
    assert.ok(c.execution);
    assert.ok(c.inputs);
    assert.ok(c.outputs);
    assert.ok(c.parameters);
  }
  for (const [key, contract] of Object.entries(semanticCatalog))
    assert.deepEqual(
      Object.keys(nodeDefinitions[key].parameters).sort(),
      Object.keys(contract.parameters).sort(),
      key,
    );
  assert.equal(CATALOG_VERSION, 7);
  assert.deepEqual(nodeDefinitions.pipeline.parameters, {
    pipeline: {
      type: "string",
      default: { kind: "string", value: "gltf_standard" },
    },
    depthCompare: {
      type: "string",
      default: { kind: "string", value: "less_equal" },
      enum: ["never", "less", "equal", "less_equal", "greater", "not_equal", "greater_equal", "always"],
    },
    depthWriteEnabled: {
      type: "boolean",
      default: { kind: "boolean", value: true },
    },
    clearDepth: {
      type: "number",
      default: { kind: "number", value: 1 },
      minimum: 0,
      maximum: 1,
    },
    clearColor: {
    type: "color",
    default: { kind: "color", value: [0.015, 0.02, 0.03, 1] },
    minimum: 0,
    maximum: 1,
    },
  });
  assert.deepEqual(nodeDefinitions.bloom_blur.parameters.direction.enum, [
    "horizontal",
    "vertical",
  ]);
  assert.deepEqual(nodeDefinitions.texture.parameters.residency.enum, [
    "transient",
    "persistent",
  ]);
  assert.deepEqual(
    nodeDefinitions.frustum_cull.parameters.cameraSelection.enum,
    ["active"],
  );
  assert.deepEqual(nodeDefinitions.mesh_query.sockets.isVisible.value.default, {
    kind: "boolean",
    value: true,
  });
  assert.equal(nodeDefinitions.mesh_query.sockets.isVisible.showValue, true);

  assert.deepEqual(nodeDefinitions.color_balance.ui.slice(0, 4), [
    { kind: "parameter", parameter: "mode" },
    { kind: "widget", widget: "grading-wheels", bindings: [
      { title: "Lift", scalar: "lift", color: "liftColor" },
      { title: "Gamma", scalar: "gamma", color: "gammaColor" },
      { title: "Gain", scalar: "gain", color: "gainColor" },
    ], visibleWhen: { parameter: "mode", equals: "lift_gamma_gain" } },
    { kind: "widget", widget: "grading-wheels", bindings: [
      { title: "Offset", scalar: "offset", color: "offsetColor" },
      { title: "Power", scalar: "power", color: "powerColor" },
      { title: "Slope", scalar: "slope", color: "slopeColor" },
    ], visibleWhen: { parameter: "mode", equals: "offset_power_slope" } },
    { kind: "parameter", parameter: "factor" },
  ]);
  for (const name of ["liftColor", "gammaColor", "gainColor", "offsetColor", "powerColor", "slopeColor"])
    assert.deepEqual(nodeDefinitions.color_balance.parameters[name].default, { kind: "color", value: [1, 1, 1, 1] });
  assert.deepEqual(nodeDefinitions.channel_mixer.parameters.redOutput, {
    type: "vector", default: { kind: "vector", value: [1, 0, 0] }, minimum: -2, maximum: 2,
  });
});

test("adapter validates and exactly lowers canonical pipeline controls and blur direction", () => {
  const x = fixture();
  const pipeline = x.nodes.find((node) => node.id === "ground");
  assert.equal(pipeline.parameters.clearColor.kind, "color");
  assert.deepEqual(
    adaptFxNodeSnapshot(x).nodes.find((node) => node.id === "ground").parameters,
    {
      pipeline: "ground_plane",
      depthCompare: "less_equal",
      depthWriteEnabled: true,
      clearDepth: 1,
      clearColor: [0.015, 0.02, 0.03, 1],
    },
  );
  const schema = nodeDefinitions.bloom_blur.parameters;
  const blur = structuredClone(x.nodes.find((node) => node.id === "frame_out"));
  blur.id = "blur";
  blur.typeId = "bloom_blur";
  blur.parameters = {
    direction: { kind: "string", value: "vertical" },
    radius: structuredClone(schema.radius.default),
  };
  blur.sockets = Object.entries(nodeDefinitions.bloom_blur.sockets).map(
    ([key, socket]) => ({
      key,
      id: `blur:${key}`,
      label: socket.title,
      direction: socket.direction,
      dataType: socket.type,
      accepts:
        socket.direction === "input"
          ? socketTypes[socket.type].acceptsFrom
          : [],
      maxIncomingLinks: socket.maxIncomingLinks,
      visible: socket.visible,
    }),
  );
  blur.typeVersion = descriptors.bloom_blur.version;
  x.nodes.push(blur);
  assert.deepEqual(
    adaptFxNodeSnapshot(x).nodes.find((node) => node.id === "blur").parameters
      .direction,
    [0, 1],
  );
  pipeline.parameters.clearColor.value[0] = 2;
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_PARAMETER",
  );
  pipeline.parameters.clearColor.value = [0.015, 0.02, 0.03, 1];
  pipeline.parameters.clearColor.value = [0, 0, 0];
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_PARAMETER",
  );
  pipeline.parameters.clearColor.value = [0.015, 0.02, 0.03, 1];
  pipeline.parameters.clearColor.value = [0, 0, Number.NaN, 1];
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_PARAMETER",
  );
  pipeline.parameters.clearColor.value = [0.015, 0.02, 0.03, 1];
  const texture = x.nodes.find((node) => node.id === "hdr");
  texture.parameters.residency.value = "unknown";
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_PARAMETER",
  );
  texture.parameters.residency.value = "transient";
  pipeline.parameters.clearDepth.value = -1;
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_PARAMETER",
  );
});

test("adapter validates and lowers disconnected query socket defaults", () => {
  const x = fixture();
  const query = x.nodes.find((node) => node.id === "query");
  const visible = query.sockets.find((socket) => socket.key === "isVisible");
  visible.defaultValue.value = false;
  const ir = adaptFxNodeSnapshot(x);
  assert.equal(visible.defaultValue.value, false);
  const parameters = ir.nodes.find((node) => node.id === "query").parameters;
  assert.equal(parameters.isVisible, undefined);
  assert.equal(parameters.visibleDefault, false);
  assert.equal(parameters.frustumCulledDefault, false);
  visible.defaultValue = { kind: "number", value: 0 };
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_SOCKET",
  );
  delete visible.defaultValue;
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) => error.code === "AUTHORING_SOCKET",
  );
});
test("adapter lowers the authoring-safe camera selector to the Rust wire field", () => {
  const ir = adaptFxNodeSnapshot(fixture());
  const parameters = ir.nodes.find((node) => node.id === "cull").parameters;
  assert.deepEqual(parameters, { camera: "active" });
  assert.equal(parameters.cameraSelection, undefined);
});
test("adapter deterministically emits the canonical schema, permits repeated types, omits muted links and maps sources", () => {
  const x = fixture(),
    a = adaptFxNodeSnapshot(x, 7);
  x.nodes.reverse();
  x.links.reverse();
  assert.deepEqual(adaptFxNodeSnapshot(x, 7), a);
  assert.equal(a.schemaVersion, 2);
  assert.equal(a.graphId, GRAPH_ID);
  assert.equal(a.nodes.filter((n) => n.executor.key === "texture").length, 2);
  assert.ok(
    Object.values(getSourceMap(a)).some((source) => source.input === "color"),
  );
  x.links.find((l) => l.id === "l_pbr_double_color_frame_out_color").muted = true;
  assert.equal(
    adaptFxNodeSnapshot(x, 8).nodes.find((n) => n.id === "frame_out").inputs
      .color,
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
  reject((x) => (x.catalogVersion = 2), "AUTHORING_CATALOG");
  reject((x) => (x.nodes[0].id = "bad id"), "AUTHORING_ID");
  reject((x) => (x.nodes[1].id = x.nodes[0].id), "AUTHORING_ID_DUPLICATE");
  reject((x) => (x.nodes[0].typeId = "wat"), "AUTHORING_NODE_TYPE");
  reject((x) => (x.nodes[0].typeVersion = 2), "AUTHORING_NODE_INVALID");
  reject((x) => (x.nodes[0].sockets = []), "AUTHORING_SOCKET_SET");
  reject((x) => (x.links[0].toSocketId = "missing:x"), "AUTHORING_LINK");
  reject((x) => {
    const link = x.links.find((l) => l.toSocketId === "frame_out:color");
    link.fromNodeId = "mesh";
    link.fromSocketId = "mesh:mesh";
  }, "AUTHORING_LINK_TYPE");
});
test("Frame Out has the exact v3 schema, defaults, UI, and strict authoring validation", () => {
  const fields = ["surfaceFormat", "hdrEnabled", "toneMapper", "exposureStops", "outputTransfer", "scaleMode", "filter", "backgroundColor"];
  assert.equal(CATALOG_VERSION, 7);
  assert.deepEqual(semanticCatalog.frame_out, {
    version: 3, execution: "frame", inputs: { color: semanticCatalog.frame_out.inputs.color }, outputs: {},
    parameters: { surfaceFormat: "preferred", hdrEnabled: true, toneMapper: "aces", exposureStops: 0, outputTransfer: "srgb", scaleMode: "stretch", filter: "linear", backgroundColor: [0, 0, 0, 1] },
  });
  assert.deepEqual(nodeDefinitions.frame_out.parameters, {
    surfaceFormat: { type: "string", default: { kind: "string", value: "preferred" }, enum: ["preferred", "rgba8_unorm", "bgra8_unorm", "rgba16_float"] },
    hdrEnabled: { type: "boolean", default: { kind: "boolean", value: true } },
    toneMapper: { type: "string", default: { kind: "string", value: "aces" }, enum: ["aces", "reinhard", "none"] },
    exposureStops: { type: "number", default: { kind: "number", value: 0 }, minimum: -10, maximum: 10 },
    outputTransfer: { type: "string", default: { kind: "string", value: "srgb" }, enum: ["srgb", "linear"] },
    scaleMode: { type: "string", default: { kind: "string", value: "stretch" }, enum: ["stretch", "contain", "cover"] },
    filter: { type: "string", default: { kind: "string", value: "linear" }, enum: ["linear", "nearest"] },
    backgroundColor: { type: "color", default: { kind: "color", value: [0, 0, 0, 1] }, minimum: 0, maximum: 1 },
  });
  assert.deepEqual(nodeDefinitions.frame_out.ui, [
    { kind: "text", variant: "section", title: "Canvas Presentation" },
    { kind: "parameter", parameter: "surfaceFormat", title: "Surface Format" },
    { kind: "text", variant: "section", title: "Display Transform" },
    { kind: "parameter", parameter: "hdrEnabled", title: "HDR" },
    { kind: "parameter", parameter: "toneMapper", title: "Tone Mapper", visibleWhen: { parameter: "hdrEnabled", equals: true } },
    { kind: "parameter", parameter: "exposureStops", title: "Exposure", visibleWhen: { parameter: "hdrEnabled", equals: true } },
    { kind: "parameter", parameter: "outputTransfer", title: "Transfer" },
    { kind: "parameter", parameter: "scaleMode", title: "Scale" },
    { kind: "parameter", parameter: "filter" },
    { kind: "parameter", parameter: "backgroundColor", title: "Background", visibleWhen: { parameter: "scaleMode", equals: "contain" } },
    { kind: "socket", socket: "color" },
  ]);
  const reject = (mutate, code, parameter) => {
    const x = fixture(), n = x.nodes.find((node) => node.typeId === "frame_out");
    mutate(x, n);
    assert.throws(() => adaptFxNodeSnapshot(x), (e) => e instanceof AuthoringGraphError && e.code === code && (!parameter || e.details.nodeId === n.id && e.details.parameter === parameter));
  };
  reject((x) => x.catalogVersion = 5, "AUTHORING_CATALOG");
  reject((x, n) => n.typeVersion = 2, "AUTHORING_NODE_INVALID");
  for (const field of fields) reject((x, n) => delete n.parameters[field], "AUTHORING_PARAMETER_SET");
  reject((x, n) => n.parameters.extra = { kind: "number", value: 0 }, "AUTHORING_PARAMETER_SET");
  for (const [field, value] of [
    ["surfaceFormat", "bad"], ["hdrEnabled", 1], ["toneMapper", "bad"], ["outputTransfer", "bad"], ["scaleMode", "bad"], ["filter", "bad"],
    ["exposureStops", NaN], ["exposureStops", -10.01], ["exposureStops", 10.01],
    ["backgroundColor", [0, 0, 0]], ["backgroundColor", [0, 0, Infinity, 1]], ["backgroundColor", [-0.01, 0, 0, 1]], ["backgroundColor", [0, 0, 0, 1.01]],
  ]) reject((x, n) => n.parameters[field].value = value, "AUTHORING_PARAMETER", field);
  for (const [hidden, value] of [["toneMapper", "bad"], ["exposureStops", Infinity]])
    reject((x, n) => { n.parameters.hdrEnabled.value = false; n.parameters[hidden].value = value; }, "AUTHORING_PARAMETER", hidden);
  reject((x, n) => { n.parameters.scaleMode.value = "stretch"; n.parameters.backgroundColor.value = [2, 0, 0, 1]; }, "AUTHORING_PARAMETER", "backgroundColor");
});
test("adapter counts only active incoming links and reports socket overflow", () => {
  const x = fixture();
  const active = x.links.find((link) => link.toSocketId === "frame_out:color");
  x.links.push({
    ...structuredClone(active),
    id: "muted_duplicate",
    muted: true,
  });
  assert.doesNotThrow(() => adaptFxNodeSnapshot(x));
  x.links.push({ ...structuredClone(active), id: "active_overflow" });
  assert.throws(
    () => adaptFxNodeSnapshot(x),
    (error) =>
      error.code === "AUTHORING_LINK_INCOMING" &&
      error.details.socketId === "frame_out:color",
  );
});
test("source map covers Rust fields, nested values, every input and is deeply frozen", () => {
  const snapshot = fixture();
  snapshot.links.find((link) => link.toSocketId === "frame_out:color").muted =
    true;
  const ir = adaptFxNodeSnapshot(snapshot, 9),
    map = getSourceMap(ir);
  for (const path of [
    "schemaVersion",
    "graphId",
    "revision",
    "nodes",
    "nodes[0].id",
    "nodes[0].state",
    "nodes[0].executor.key",
    "nodes[0].executor.version",
    "nodes[0].parameters",
    "nodes[0].inputs",
  ])
    assert.ok(map[path], path);
  for (const [index, node] of ir.nodes.entries())
    for (const input of Object.keys(semanticCatalog[node.executor.key].inputs))
      assert.ok(map[`nodes[${index}].inputs.${input}`]);
  assert.ok(
    Object.keys(map).some((path) =>
      /parameters\..+\[|parameters\..+\..+/.test(path),
    ),
  );
  const socket = Object.values(map).find((source) => source.kind === "socket");
  const unconnected = Object.values(map).find(
    (source) => source.unconnected === true,
  );
  const link = Object.values(map).find((source) => source.kind === "link");
  assert.ok(socket?.socketId && unconnected?.socketId);
  assert.equal(
    ir.nodes.find((node) => node.id === "frame_out").inputs.source,
    undefined,
  );
  for (const field of [
    "linkId",
    "fromNodeId",
    "fromSocketId",
    "toNodeId",
    "toSocketId",
    "muted",
  ])
    assert.ok(Object.hasOwn(link, field), field);
  assert.ok(Object.isFrozen(map) && Object.isFrozen(link));
});
test("texture source maps identify every flat authored control", () => {
  const snapshot = fixture();
  const hdr = snapshot.nodes.find((node) => node.id === "hdr");
  hdr.parameters.viewFormat.value = "rgba16_float";
  const ir = adaptFxNodeSnapshot(snapshot);
  const index = ir.nodes.findIndex((node) => node.id === "hdr");
  const root = `nodes[${index}].parameters`;
  const map = getSourceMap(ir);
  const source = (parameter) => ({
    kind: "parameter",
    nodeId: "hdr",
    parameter,
  });
  assert.deepEqual(map[`${root}.residency`], source("residency"));
  assert.deepEqual(map[`${root}.texture`], { kind: "node", nodeId: "hdr" });
  for (const [path, parameter] of [
    ["dimension", "dimension"],
    ["format", "format"],
    ["extent", "extentMode"],
    ["extent.kind", "extentMode"],
    ["extent.depthOrArrayLayers", "depthOrArrayLayers"],
    ["extent.width", "extentMode"],
    ["extent.width.numerator", "relativeWidthNumerator"],
    ["extent.width.denominator", "relativeWidthDenominator"],
    ["extent.height", "extentMode"],
    ["extent.height.numerator", "relativeHeightNumerator"],
    ["extent.height.denominator", "relativeHeightDenominator"],
    ["mipLevelCount", "mipLevelCount"],
    ["sampleCount", "sampleCount"],
    ["viewFormats", "viewFormat"],
    ["viewFormats[0]", "viewFormat"],
  ])
    assert.deepEqual(map[`${root}.texture.${path}`], source(parameter), path);
  assert.ok(!Object.values(map).some((value) => value.parameter === "texture"));

  const absolute = fixture();
  absolute.nodes.find((node) => node.id === "hdr").parameters.extentMode.value =
    "absolute";
  const absoluteIr = adaptFxNodeSnapshot(absolute);
  const absoluteIndex = absoluteIr.nodes.findIndex((node) => node.id === "hdr");
  const absoluteMap = getSourceMap(absoluteIr);
  assert.deepEqual(
    absoluteMap[`nodes[${absoluteIndex}].parameters.texture.extent.width`],
    source("absoluteWidth"),
  );
  assert.deepEqual(
    absoluteMap[`nodes[${absoluteIndex}].parameters.texture.extent.height`],
    source("absoluteHeight"),
  );
});
test("unsupported texture diagnostics map to their exact authored controls", () => {
  const ir = adaptFxNodeSnapshot(fixture());
  const index = ir.nodes.findIndex((node) => node.id === "hdr");
  for (const [suffix, parameter] of [
    ["dimension", "dimension"],
    ["mipLevelCount", "mipLevelCount"],
    ["sampleCount", "sampleCount"],
    ["extent.depthOrArrayLayers", "depthOrArrayLayers"],
  ]) {
    const path = `nodes[${index}].parameters.texture.${suffix}`;
    const mapped = mapAuthoringDiagnostic(
      ir,
      new RendererError("GRAPH_UNSUPPORTED_FEATURE", {
        message: "unsupported",
        path,
      }),
    );
    assert.equal(mapped.path, path);
    assert.deepEqual(mapped.source, {
      kind: "parameter",
      nodeId: "hdr",
      parameter,
    });
  }
});
test("diagnostic mapper creates a frozen RendererError DTO with fallbacks and prefix matching", () => {
  const ir = adaptFxNodeSnapshot(fixture());
  const original = new RendererError("GRAPH_INPUT", {
    message: "bad",
    field: "nodes[0].executor.key.more",
    nested: { x: 1 },
  });
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
  const original = new RendererError("GRAPH_BAD", {
    path: "nodes[0].id",
    message: "bad",
  });
  const states = [];
  const c = new AuthoringController({
    adapt: (snapshot, revision) => adaptFxNodeSnapshot(snapshot, revision),
    renderer: {
      compileGraph: async () => {
        throw original;
      },
      switchCompiledGraph: async () => {},
      dropCompiledGraph: async () => {},
    },
  });
  c.subscribe((state) => states.push(state));
  c.markDirty(fixture());
  await assert.rejects(c.apply(), (error) => error === original);
  assert.notStrictEqual(states.at(-1).error, original);
  let subscribed;
  c.subscribe((state) => {
    subscribed = state;
  })();
  assert.strictEqual(subscribed.error, states.at(-1).error);
  await c.destroy();
});
test("apply after destroy does not compile and destroy returns one strict promise", async () => {
  let compiles = 0;
  const c = new AuthoringController({
    adapt: () => ({}),
    renderer: {
      compileGraph: async () => {
        compiles++;
      },
      dropCompiledGraph: async () => {},
      switchCompiledGraph: async () => {},
    },
  });
  c.markDirty({});
  const first = c.destroy();
  assert.strictEqual(c.destroy(), first);
  assert.strictEqual(await c.apply(), null);
  await first;
  assert.equal(compiles, 0);
});
