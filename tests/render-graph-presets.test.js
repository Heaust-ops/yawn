import test from "node:test";
import assert from "node:assert/strict";
import * as presets from "../static/render-graph/presets.js";
import { descriptors } from "../static/render-graph/catalog.js";

test("all presets use current schemas, versions, and one frame output", () => {
  assert.equal(Object.keys(presets.renderGraphPresets).length, 13);
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    assert.deepEqual([graph.schemaVersion, graph.revision], [3, 4], name);
    assert.equal(new Set(graph.nodes.map((node) => node.id)).size, graph.nodes.length, name);
    assert.equal(graph.nodes.filter((node) => node.executor.key === "frame_out").length, 1, name);
    for (const node of graph.nodes)
      assert.equal(node.executor.version, descriptors[node.executor.key].version, `${name}:${node.id}`);
  }
  const authoredX4 = Object.entries(presets.renderGraphPresets).flatMap(([name, graph]) =>
    graph.nodes.filter((node) => node.parameters?.texture?.sampleCount === 4)
      .map((node) => `${name}:${node.id}`));
  assert.deepEqual(authoredX4, ["msaa:msaa_hdr", "msaa:scene_depth"]);
  assert.equal(typeof presets.msaa.nodes.find((node) => node.id === "msaa_hdr")
    .parameters.texture.sampleCount, "number");
});

test("presets classify demo-owned enable and material bits through type.words[0] predicates", () => {
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    const byId = Object.fromEntries(graph.nodes.map((node) => [node.id, node]));
    assert.deepEqual(byId.type_words.inputs.value, [{ node: "mesh", socket: "type" }], name);
    assert.deepEqual(byId.type_bits.inputs.value, [{ node: "type_words", socket: "word0" }], name);
    assert.deepEqual(byId.ground.inputs.predicate, [{ node: "ground_class", socket: "value" }], name);
    assert.deepEqual(byId.pbr.inputs.predicate, [{ node: "standard_class", socket: "value" }], name);
    assert.deepEqual(byId.pbr_double.inputs.predicate, [{ node: "double_class", socket: "value" }], name);
    for (const pipeline of [byId.ground, byId.pbr, byId.pbr_double]) {
      assert.deepEqual(pipeline.inputs.mesh, [{ node: "mesh", socket: "mesh" }]);
      assert.equal(pipeline.executor.version, 2);
    }
  }
});

test("culling adds a local-AABB expression to each material predicate", () => {
  const byId = Object.fromEntries(presets.culling.nodes.map((node) => [node.id, node]));
  assert.deepEqual(byId.cull.inputs, {
    mesh: [{ node: "mesh", socket: "mesh" }], localAabb: [{ node: "mesh", socket: "localAabb" }],
  });
  assert.deepEqual(byId.not_culled.inputs.operand, [{ node: "cull", socket: "isFrustumCulled" }]);
  for (const id of ["ground_class", "standard_class", "double_class"])
    assert.equal(byId[id].inputs.inputs.at(-1).node, "not_culled");
});

test("scene pipelines directly share matching explicit color and depth targets", () => {
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    const byId = Object.fromEntries(graph.nodes.map((node) => [node.id, node]));
    const color = byId.ground.inputs.color;
    const depth = byId.ground.inputs.depth;
    for (const id of ["ground", "pbr", "pbr_double"]) {
      assert.deepEqual(byId[id].inputs.color, color, `${name}:${id}`);
      assert.deepEqual(byId[id].inputs.depth, depth, `${name}:${id}`);
      assert.equal(["ground", "pbr", "pbr_double"].includes(color[0].node), false, name);
    }
    const colorDescriptor = byId[color[0].node].parameters.texture;
    const depthDescriptor = byId[depth[0].node].parameters.texture;
    assert.equal(depthDescriptor.format, "depth32_float", name);
    assert.deepEqual(depthDescriptor.extent, colorDescriptor.extent, name);
    assert.equal(depthDescriptor.sampleCount, colorDescriptor.sampleCount, name);
  }
});
