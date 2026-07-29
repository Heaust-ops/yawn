import test from "node:test";
import assert from "node:assert/strict";
import { culling } from "../static/render-graph/presets.js";
import {
  CATALOG_VERSION, GRAPH_ID, semanticCatalog, nodeDefinitions, descriptors, socketTypes,
} from "../static/render-graph/catalog.js";
import { adaptFxNodeSnapshot, mapAuthoringDiagnostic } from "../static/render-graph/adapter.js";

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

test("catalog v11 exposes the final mesh, pipeline, and typed-expression contracts", () => {
  assert.equal(CATALOG_VERSION, 11);
  assert.deepEqual(semanticCatalog.mesh.outputs, {
    mesh: { type: "mesh_data" }, type: { type: "u32x16" }, localAabb: { type: "local_aabb" },
  });
  assert.equal(semanticCatalog.mesh.version, 2);
  assert.equal(nodeDefinitions.mesh.sockets.localAabb.title, "Local AABB");
  assert.equal(semanticCatalog.pipeline.version, 4);
  assert.deepEqual(semanticCatalog.pipeline.inputs.predicate.cardinality, { minimum: 0, maximum: 1 });
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

test("current culling fixture uses type-bit predicates and final socket versions", () => {
  const byId = Object.fromEntries(culling.nodes.map((node) => [node.id, node]));
  assert.deepEqual(byId.cull.inputs.localAabb, [{ node: "mesh", socket: "localAabb" }]);
  assert.deepEqual(byId.type_words.inputs.value, [{ node: "mesh", socket: "type" }]);
  assert.equal(byId.ground.executor.version, 4);
  assert.equal(byId.ground.inputs.predicate[0].node, "ground_class");
});

test("compiler texture sockets expose policy metadata without literal widgets", () => {
  for (const socket of ["colorTarget", "depthTarget"]) {
    assert.equal(semanticCatalog.pipeline.inputs[socket].defaultPolicy, "compiler_texture");
    assert.equal(nodeDefinitions.pipeline.sockets[socket].default, undefined);
  }
  assert.equal(semanticCatalog.pipeline.inputs.predicate.defaultPolicy, "parameter_literal");
  assert.notEqual(nodeDefinitions.pipeline.sockets.predicate.default, null);
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
  assert.equal(graph.schemaVersion, 3);
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
