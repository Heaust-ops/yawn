import test from "node:test";
import assert from "node:assert/strict";
import * as presets from "../static/render-graph/presets.js";
import { descriptors } from "../static/render-graph/catalog.js";

test("all presets use current schemas, versions, and one frame output", () => {
  assert.equal(Object.keys(presets.renderGraphPresets).length, 12);
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    assert.deepEqual([graph.schemaVersion, graph.revision], [2, 1], name);
    assert.equal(new Set(graph.nodes.map((node) => node.id)).size, graph.nodes.length, name);
    assert.equal(graph.nodes.filter((node) => node.executor.key === "frame_out").length, 1, name);
    for (const node of graph.nodes)
      assert.equal(node.executor.version, descriptors[node.executor.key].version, `${name}:${node.id}`);
  }
});

test("presets classify visibility and material through type.words[0] predicates", () => {
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    const byId = Object.fromEntries(graph.nodes.map((node) => [node.id, node]));
    assert.deepEqual(byId.type_words.inputs.value, { node: "mesh", socket: "type" }, name);
    assert.deepEqual(byId.type_bits.inputs.value, { node: "type_words", socket: "word0" }, name);
    const suffix = name === "culling" ? "_final" : "_class";
    assert.deepEqual(byId.ground.inputs.predicate, { node: `ground${suffix}`, socket: "value" }, name);
    assert.deepEqual(byId.pbr.inputs.predicate, { node: name === "culling" ? "pbr_final" : "standard_class", socket: "value" }, name);
    assert.deepEqual(byId.pbr_double.inputs.predicate, { node: name === "culling" ? "pbr_double_final" : "double_class", socket: "value" }, name);
    for (const pipeline of [byId.ground, byId.pbr, byId.pbr_double]) {
      assert.deepEqual(pipeline.inputs.mesh, { node: "mesh", socket: "mesh" });
      assert.equal(pipeline.executor.version, 3);
    }
  }
});

test("culling adds a local-AABB expression to each material predicate", () => {
  const byId = Object.fromEntries(presets.culling.nodes.map((node) => [node.id, node]));
  assert.deepEqual(byId.cull.inputs, {
    mesh: { node: "mesh", socket: "mesh" }, localAabb: { node: "mesh", socket: "localAabb" },
  });
  assert.deepEqual(byId.not_culled.inputs.operand, { node: "cull", socket: "isFrustumCulled" });
  for (const id of ["ground", "pbr", "pbr_double"])
    assert.equal(byId[id].inputs.predicate.node.endsWith("_final"), true);
});

test("implicit presets start disconnected and then chain both attachments", () => {
  for (const name of ["hdr", "culling", "grading"]) {
    const byId = Object.fromEntries(presets[name].nodes.map((node) => [node.id, node]));
    assert.equal("colorTarget" in byId.ground.inputs, false, name);
    assert.equal("depthTarget" in byId.ground.inputs, false, name);
    assert.deepEqual(byId.pbr.inputs.colorTarget, { node: "ground", socket: "color" }, name);
    assert.deepEqual(byId.pbr.inputs.depthTarget, { node: "ground", socket: "depth" }, name);
  }
  const midnight = Object.fromEntries(presets.midnight.nodes.map((node) => [node.id, node]));
  assert.deepEqual(midnight.ground.inputs.colorTarget, { node: "ldr", socket: "texture" });
});
