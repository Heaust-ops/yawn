import test from "node:test";
import assert from "node:assert/strict";
import { culling } from "../examples/render-graph-studio/render-graph/presets.js";
import {
  CATALOG_VERSION, GRAPH_ID, semanticCatalog, nodeDefinitions, descriptors, socketTypes,
} from "@yawn/render-graph-fxnode/catalog";
import { adaptFxNodeSnapshot, mapAuthoringDiagnostic } from "@yawn/render-graph-fxnode";

const authoredNode = (id, typeId) => {
  const definition = nodeDefinitions[typeId];
  return {
    id, typeId, typeVersion: definition.version,
    position: { x: 0, y: 0 }, size: { x: 200, y: 120 }, label: id,
    parameters: Object.fromEntries(Object.entries(definition.parameters)
      .map(([key, schema]) => [key, structuredClone(schema.default)])),
    sockets: Object.entries(definition.sockets).map(([key, socket]) => ({
      id: `${id}:${key}`, key, label: socket.title, direction: socket.direction,
      dataType: socket.type,
      accepts: socket.direction === "input" ? [...socketTypes[socket.type].acceptsFrom] : [],
      maxIncomingLinks: socket.maxIncomingLinks,
      ...(socket.value ? { defaultValue: structuredClone(socket.value.default) } : {}),
      visible: socket.visible,
    })),
    muted: false, collapsed: false, extensions: {}, known: true,
  };
};

const authoredLink = (id, from, to = "target", muted = false) => ({
  id, fromNodeId: from, fromSocketId: `${from}:value`,
  toNodeId: to, toSocketId: `${to}:inputs`, muted, extensions: {},
});

test("catalog v13 exposes raster and typed-expression contracts", () => {
  assert.equal(CATALOG_VERSION, 13);
  assert.deepEqual(semanticCatalog.mesh.outputs, {
    mesh: { type: "mesh_data" }, type: { type: "u32x16" }, localAabb: { type: "local_aabb" },
  });
  assert.equal(semanticCatalog.mesh.version, 2);
  assert.equal(nodeDefinitions.mesh.sockets.localAabb.title, "Local AABB");
  assert.equal(semanticCatalog.pipeline, undefined);
  for (const key of ["ground_plane", "gltf_standard", "gltf_standard_double_sided"]) {
    assert.equal(semanticCatalog[key].version, 2);
    assert.deepEqual(semanticCatalog[key].inputs.predicate.cardinality, { minimum: 0, maximum: 1 });
  }
  assert.deepEqual(semanticCatalog.and.inputs.inputs.cardinality, { minimum: 0, maximum: 8 });
  assert.equal(nodeDefinitions.and.sockets.inputs.maxIncomingLinks, 8);
  assert.equal(nodeDefinitions.and.sockets.inputs.value, null);
  assert.deepEqual(nodeDefinitions.texture.parameters.sampleCount.enum, ["1", "4"]);
  for (const key of ["and", "xnor", "equals_f32", "greater_than_u32", "combine_vec4",
    "separate_mat4", "combine_u32_bits", "separate_u32x16", "separate_local_aabb"])
    assert.equal(semanticCatalog[key].execution, "expression", key);
  for (const [key, contract] of Object.entries(semanticCatalog)) {
    assert.equal(nodeDefinitions[key].version, contract.version, key);
    assert.equal(descriptors[key].version, contract.version, key);
  }
});

test("raster declarations own their defaults and sockets", () => {
  const keys = ["ground_plane", "gltf_standard", "gltf_standard_double_sided"];
  assert.deepEqual(keys.map((key) => nodeDefinitions[key].title), [
    "Ground Plane", "glTF Standard", "glTF Standard — Double-Sided",
  ]);
  assert.notStrictEqual(semanticCatalog.ground_plane.inputs, semanticCatalog.gltf_standard.inputs);
  assert.notStrictEqual(nodeDefinitions.ground_plane.sockets, nodeDefinitions.gltf_standard.sockets);
  assert.notStrictEqual(nodeDefinitions.ground_plane.parameters, nodeDefinitions.gltf_standard.parameters);
  assert.notStrictEqual(nodeDefinitions.ground_plane.parameters.clearColor.default.value,
    nodeDefinitions.gltf_standard.parameters.clearColor.default.value);
  assert.equal("pipeline" in nodeDefinitions.gltf_standard.parameters, false);
  assert.equal(["draw", "Order"].join("") in nodeDefinitions.gltf_standard.parameters, false);
  assert.deepEqual(Object.keys(semanticCatalog.gltf_standard.inputs), [
    "mesh", "predicate", "input.color", "input.depth",
  ]);
  assert.deepEqual(Object.keys(semanticCatalog.gltf_standard.outputs), [
    "output.color", "output.depth",
  ]);
  for (const [identity, direction, semanticName] of [
    ["input.color", "input", "color"],
    ["output.color", "output", "color"],
    ["input.depth", "input", "depth"],
    ["output.depth", "output", "depth"],
  ]) {
    assert.equal(nodeDefinitions.gltf_standard.sockets[identity].direction, direction);
    assert.equal(nodeDefinitions.gltf_standard.sockets[identity].title, semanticName);
  }
});

test("current culling fixture uses type-bit predicates and final socket versions", () => {
  const byId = Object.fromEntries(culling.nodes.map((node) => [node.id, node]));
  assert.deepEqual(byId.cull.inputs.localAabb, [{ node: "mesh", socket: "localAabb" }]);
  assert.deepEqual(byId.type_words.inputs.value, [{ node: "mesh", socket: "type" }]);
  assert.equal(byId.ground.executor.version, 2);
  assert.equal(byId.ground.inputs.predicate[0].node, "ground_class");
});

test("compiler texture sockets expose policy metadata without literal widgets", () => {
  for (const socket of ["input.color", "input.depth"]) {
    assert.equal(semanticCatalog.gltf_standard.inputs[socket].defaultPolicy, "compiler_texture");
    assert.equal(nodeDefinitions.gltf_standard.sockets[socket].default, undefined);
  }
  assert.equal(semanticCatalog.gltf_standard.inputs.predicate.defaultPolicy, "parameter_literal");
  assert.notEqual(nodeDefinitions.gltf_standard.sockets.predicate.default, null);
});

test("removed architecture is absent from the authoring catalog", () => {
  for (const removed of ["mesh_query", "pipeline_registry"])
    assert.equal(semanticCatalog[removed], undefined);
  const serialized = JSON.stringify(semanticCatalog);
  for (const removedSocket of ["isVisible", "localAabbs", "activation"])
    assert.equal(serialized.includes(`\"${removedSocket}\"`), false);
});

test("adapter preserves ordered multisocket links and indexed diagnostics", () => {
  const raw = {
    graphId: GRAPH_ID, catalogVersion: CATALOG_VERSION,
    nodes: ["source_a", "source_b", "source_c"].map((id) => authoredNode(id, "not"))
      .concat(authoredNode("target", "and")),
    links: [
      authoredLink("link_a", "source_a"),
      authoredLink("link_muted", "source_c", "target", true),
      authoredLink("link_b", "source_b"),
    ],
    metadata: {}, version: 1,
  };
  const graph = adaptFxNodeSnapshot(raw, 2);
  const targetIndex = graph.nodes.findIndex((node) => node.id === "target");
  assert.deepEqual([graph.kind, graph.version, graph.id], ["yawn-render-graph", 1, GRAPH_ID]);
  assert.deepEqual(graph.nodes[targetIndex].inputs.inputs, [
    { node: "source_a", socket: "value" },
    { node: "source_b", socket: "value" },
  ]);
  const diagnostic = mapAuthoringDiagnostic(graph, {
    code: "GRAPH_SOCKET_TYPE_MISMATCH",
    details: { path: `nodes[${targetIndex}].inputs.inputs[1].node` },
  });
  assert.equal(diagnostic.source.linkId, "link_b");
});

test("adapter lowers direction-qualified raster sockets and maps diagnostics", () => {
  const link = (id, fromNodeId, fromKey, toNodeId, toKey) => ({
    id,
    fromNodeId,
    fromSocketId: `${fromNodeId}:${fromKey}`,
    toNodeId,
    toSocketId: `${toNodeId}:${toKey}`,
    muted: false,
    extensions: {},
  });
  const raw = {
    graphId: GRAPH_ID,
    catalogVersion: CATALOG_VERSION,
    nodes: [
      authoredNode("texture", "texture"),
      authoredNode("mesh", "mesh"),
      authoredNode("raster", "gltf_standard"),
      authoredNode("target", "frame_out"),
    ],
    links: [
      link("mesh_link", "mesh", "mesh", "raster", "mesh"),
      link("target_link", "texture", "texture", "raster", "input.color"),
      link("frame_link", "raster", "output.color", "target", "color"),
    ],
    metadata: {},
    version: 1,
  };

  const graph = adaptFxNodeSnapshot(raw, 2);
  const rasterIndex = graph.nodes.findIndex((node) => node.id === "raster");
  const targetIndex = graph.nodes.findIndex((node) => node.id === "target");
  assert.deepEqual(graph.nodes[rasterIndex].inputs.color, [
    { node: "texture", socket: "texture" },
  ]);
  assert.deepEqual(graph.nodes[targetIndex].inputs.color, [
    { node: "raster", socket: "color" },
  ]);
  assert.equal(
    mapAuthoringDiagnostic(graph, {
      code: "GRAPH_INPUT_CARDINALITY",
      details: { path: `nodes[${rasterIndex}].inputs.depth` },
    }).source.socketId,
    "raster:input.depth",
  );
  assert.equal(
    mapAuthoringDiagnostic(graph, {
      code: "GRAPH_UNKNOWN_SOCKET",
      details: { path: `nodes[${targetIndex}].inputs.color[0].socket` },
    }).source.socketId,
    "raster:output.color",
  );
});
