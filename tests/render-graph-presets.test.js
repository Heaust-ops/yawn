import test from "node:test";
import assert from "node:assert/strict";
import {
  culling,
  ember,
  hdr,
  midnight,
  renderGraphPresets,
} from "../static/render-graph/presets.js";
test("presets use canonical node graphs", () => {
  assert.deepEqual(Object.keys(renderGraphPresets), [
    "midnight",
    "ember",
    "hdr",
    "culling",
    "tone",
    "edges",
    "bloom",
    "combined",
  ]);
  assert.deepEqual(
    [midnight.graphId, ember.graphId],
    ["preset_midnight", "preset_ember"],
  );
  assert.notDeepEqual(midnight.nodes[6].parameters.clearColor, ember.nodes[6].parameters.clearColor);
  for (const graph of [midnight, ember]) {
    assert.equal(graph.schemaVersion, 2);
    assert.equal(graph.revision, 1);
    assert.equal(graph.nodes[6].executor.key, "legacy_forward");
    assert.deepEqual(graph.nodes[6].inputs.colorTarget, { node: "surface", socket: "surface" });
    assert.equal(graph.nodes.at(-1).executor.key, "present");
  }
  assert.equal(hdr.schemaVersion, 2);
  assert.equal(hdr.revision, 1);
  assert.equal(hdr.graphId, "preset_hdr_fullscreen");
  assert.equal(
    new Set(Object.values(renderGraphPresets).map((graph) => graph.graphId))
      .size,
    8,
  );
  assert.deepEqual(
    hdr.nodes.map((node) => [node.id, node.executor.key]),
    [
      ["surface", "surface_target"],
      ["hdr", "texture_spec"],
      ["depth", "texture_spec"],
      ["scene", "scene_table"],
      ["visible", "visibility_flags"],
      ["query", "mesh_query"],
      ["depth_config", "depth_stencil_config"],
      ["forward", "legacy_forward"],
      ["copy", "fullscreen_copy"],
      ["present", "present"],
    ],
  );
  const byId = Object.fromEntries(hdr.nodes.map((node) => [node.id, node]));
  assert.equal(byId.hdr.parameters.texture.format, "rgba16_float");
  assert.deepEqual(byId.query.inputs, {
    scene: { node: "scene", socket: "scene" },
    isVisible: { node: "visible", socket: "flags" },
  });
  assert.deepEqual(byId.query.parameters.filters, [
    { flag: "isVisible", predicate: "required_true" },
    { flag: "isFrustumCulled", predicate: "any" },
  ]);
  assert.deepEqual(byId.forward.inputs.colorTarget, {
    node: "hdr",
    socket: "spec",
  });
  assert.deepEqual(byId.copy.executor, { key: "fullscreen_copy", version: 1 });
  assert.deepEqual(byId.copy.inputs, {
    source: { node: "forward", socket: "color" },
    colorTarget: { node: "surface", socket: "surface" },
  });
  assert.deepEqual(byId.present.inputs.surface, {
    node: "copy",
    socket: "color",
  });
  assert.deepEqual(
    culling.nodes
      .filter((node) => ["frustum_cull", "mesh_query"].includes(node.executor.key))
      .map((node) => node.executor.key),
    ["frustum_cull", "mesh_query"],
  );
  const cullingQuery = culling.nodes.find((node) => node.id === "query");
  assert.equal(cullingQuery.parameters.filters[1].predicate, "required_false");
  assert.deepEqual(cullingQuery.inputs.isFrustumCulled, {
    node: "cull",
    socket: "flags",
  });
});
