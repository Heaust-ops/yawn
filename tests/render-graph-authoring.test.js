import test from "node:test";
import assert from "node:assert/strict";
import { culling } from "../static/render-graph/presets.js";
import {
  CATALOG_VERSION, semanticCatalog, nodeDefinitions, descriptors,
} from "../static/render-graph/catalog.js";

test("catalog v10 exposes the final mesh, pipeline, and typed-expression contracts", () => {
  assert.equal(CATALOG_VERSION, 10);
  assert.deepEqual(semanticCatalog.mesh.outputs, {
    mesh: { type: "mesh_data" }, type: { type: "u32x16" }, localAabb: { type: "local_aabb" },
  });
  assert.equal(semanticCatalog.mesh.version, 2);
  assert.equal(nodeDefinitions.mesh.sockets.localAabb.title, "Local AABB");
  assert.equal(semanticCatalog.pipeline.version, 4);
  assert.equal(semanticCatalog.pipeline.inputs.predicate.required, false);
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
  assert.deepEqual(byId.cull.inputs.localAabb, { node: "mesh", socket: "localAabb" });
  assert.deepEqual(byId.type_words.inputs.value, { node: "mesh", socket: "type" });
  assert.equal(byId.ground.executor.version, 4);
  assert.equal(byId.ground.inputs.predicate.node, "ground_final");
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
