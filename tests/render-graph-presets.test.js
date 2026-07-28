import test from "node:test";
import assert from "node:assert/strict";
import * as presets from "../static/render-graph/presets.js";
import { descriptors } from "../static/render-graph/catalog.js";

const order = [
  "midnight",
  "ember",
  "hdr",
  "culling",
  "tone",
  "contain",
  "reinhard",
  "linear",
  "grading",
  "edges",
  "bloom",
  "combined",
];
const sequences = {
  midnight: [
    ["ldr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["frame_out", "frame_out"],
  ],
  ember: [
    ["ldr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["frame_out", "frame_out"],
  ],
  hdr: [
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["frame_out", "frame_out"],
  ],
  culling: [
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["cull", "frustum_cull"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["frame_out", "frame_out"],
  ],
  tone: [
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["frame_out", "frame_out"],
  ],
  grading: [
    ["balance_hdr", "texture"], ["exposure_hdr", "texture"], ["saturation_hdr", "texture"], ["mixer_hdr", "texture"],
    ["hdr", "texture"], ["depth", "texture"], ["mesh", "mesh"], ["query", "mesh_query"], ["registry", "pipeline_registry"],
    ["ground", "pipeline"], ["pbr", "pipeline"], ["pbr_double", "pipeline"], ["balance", "color_balance"], ["exposure", "exposure_contrast"],
    ["saturation", "saturation"], ["mixer", "channel_mixer"], ["frame_out", "frame_out"],
  ],
  edges: [
    ["edge_hdr", "texture"],
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["edges", "luminance_edge"],
    ["frame_out", "frame_out"],
  ],
  bloom: [
    ["half_a", "texture"],
    ["half_b", "texture"],
    ["half_c", "texture"],
    ["composite_hdr", "texture"],
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["extract", "bloom_extract"],
    ["blur_h", "bloom_blur"],
    ["blur_v", "bloom_blur"],
    ["composite", "bloom_composite"],
    ["frame_out", "frame_out"],
  ],
  combined: [
    ["edge_hdr", "texture"],
    ["half_a", "texture"],
    ["half_b", "texture"],
    ["half_c", "texture"],
    ["composite_hdr", "texture"],
    ["hdr", "texture"],
    ["depth", "texture"],
    ["mesh", "mesh"],
    ["query", "mesh_query"],
    ["registry", "pipeline_registry"],
    ["ground", "pipeline"],
    ["pbr", "pipeline"],
    ["pbr_double", "pipeline"],
    ["extract", "bloom_extract"],
    ["blur_h", "bloom_blur"],
    ["blur_v", "bloom_blur"],
    ["composite", "bloom_composite"],
    ["edges", "luminance_edge"],
    ["frame_out", "frame_out"],
  ],
};
for (const name of ["contain", "reinhard", "linear"])
  sequences[name] = sequences.tone;

test("presets have the exact canonical pipeline identities, schemas, and node sequences", () => {
  assert.deepEqual(Object.keys(presets.renderGraphPresets), order);
  assert.deepEqual(
    order.map((name) => presets[name].graphId),
    [
      "preset_midnight",
      "preset_ember",
      "preset_hdr_fullscreen",
      "preset_gpu_culling",
      "preset_tone",
      "preset_contain",
      "preset_reinhard",
      "preset_linear",
      "preset_grading",
      "preset_edges",
      "preset_bloom",
      "preset_combined",
    ],
  );
  for (const name of order) {
    const graph = presets[name];
    assert.deepEqual([graph.schemaVersion, graph.revision], [2, 1]);
    assert.equal(
      new Set(graph.nodes.map((node) => node.id)).size,
      graph.nodes.length,
    );
    assert.deepEqual(
      graph.nodes.map((node) => [node.id, node.executor.key]),
      sequences[name],
    );
    assert.equal(
      graph.nodes.filter((node) => node.executor.key === "frame_out").length,
      1,
    );
    assert.ok(
      graph.nodes.every(
        (node) => !["surface_target", "present"].includes(node.executor.key),
      ),
    );
    assert.ok(!graph.nodes.some((node) => node.id === "copy"));
    assert.ok(
      graph.nodes.every(
        (node) => node.executor.version === descriptors[node.executor.key].version,
      ),
    );
  }
  const grading = presets.grading;
  assert.deepEqual(grading.nodes.find((n) => n.id === "balance").parameters, {
    mode: "lift_gamma_gain", factor: 1, lift: 0, liftColor: [1,1,1,1],
    gamma: 1, gammaColor: [1,1,1,1], gain: 1, gainColor: [1,1,1,1],
    offset: 0, offsetColor: [1,1,1,1], power: 1, powerColor: [1,1,1,1],
    slope: 1, slopeColor: [1,1,1,1],
  });
  assert.deepEqual(grading.nodes.find((n) => n.id === "exposure").parameters, { exposureStops: 0, contrast: 1, pivot: .18, factor: 1 });
  assert.deepEqual(grading.nodes.find((n) => n.id === "saturation").parameters, { saturation: 1, factor: 1 });
  assert.deepEqual(grading.nodes.find((n) => n.id === "mixer").parameters, { redOutput: [1,0,0], greenOutput: [0,1,0], blueOutput: [0,0,1], factor: 1 });
});

test("presets preserve common mesh, texture, query, pipeline, culling and post wiring", () => {
  const removed = [
    "texture_spec",
    "scene_table",
    "local_aabb_buffer",
    "camera_frustum",
    "visibility_flags",
  ];
  for (const [name, graph] of Object.entries(presets.renderGraphPresets)) {
    const byId = Object.fromEntries(graph.nodes.map((node) => [node.id, node]));
    assert.deepEqual(byId.query.parameters, {
      visiblePredicate: "required_true",
      visibleDefault: true,
      frustumCulledPredicate: name === "culling" ? "required_false" : "any",
      frustumCulledDefault: false,
    });
    assert.deepEqual(byId.query.inputs.mesh, { node: "mesh", socket: "mesh" });
    assert.deepEqual(byId.query.inputs.isVisible, {
      node: "mesh",
      socket: "isVisible",
    });
    assert.deepEqual(byId.ground.inputs.mesh, {
      node: "mesh",
      socket: "mesh",
    });
    assert.deepEqual(byId.ground.inputs.draws, {
      node: "query",
      socket: "draws",
    });
    assert.deepEqual(byId.ground.inputs.depthTarget, {
      node: "depth",
      socket: "texture",
    });
    assert.equal(
      graph.nodes.filter((node) => node.executor.key === "pipeline_registry")
        .length,
      1,
    );
    const pipelines = graph.nodes.filter(
      (node) => node.executor.key === "pipeline",
    );
    assert.deepEqual(
      pipelines.map((node) => node.parameters.pipeline),
      ["ground_plane", "gltf_standard", "gltf_standard_double_sided"],
    );
    for (const pipeline of pipelines)
      assert.deepEqual(pipeline.inputs.activation, {
        node: "registry",
        socket: "activation",
      });
    assert.deepEqual(byId.pbr.inputs.colorTarget, {
      node: "ground",
      socket: "color",
    });
    assert.deepEqual(byId.pbr.inputs.depthTarget, {
      node: "ground",
      socket: "depth",
    });
    assert.deepEqual(byId.pbr_double.inputs.colorTarget, {
      node: "pbr",
      socket: "color",
    });
    assert.deepEqual(byId.pbr_double.inputs.depthTarget, {
      node: "pbr",
      socket: "depth",
    });
    assert.ok(
      graph.nodes
        .filter((node) => node.executor.key === "texture")
        .every((node) => node.parameters.texture.dimension === "d2"),
    );
    assert.ok(
      graph.nodes.every((node) => !removed.includes(node.executor.key)),
    );
  }
  const cull = Object.fromEntries(
    presets.culling.nodes.map((node) => [node.id, node]),
  );
  assert.deepEqual(cull.cull.parameters, { camera: "active" });
  assert.deepEqual(cull.cull.inputs, {
    mesh: { node: "mesh", socket: "mesh" },
    localAabbs: { node: "mesh", socket: "localAabbs" },
  });
  assert.deepEqual(cull.query.inputs.isFrustumCulled, {
    node: "cull",
    socket: "isFrustumCulled",
  });
  for (const name of ["tone", "contain", "reinhard", "linear", "grading", "edges", "bloom", "combined"])
    assert.ok(!presets[name].nodes.some((node) => node.id === "copy"));
  const finalSource = {
    hdr: "pbr_double",
    culling: "pbr_double",
    tone: "pbr_double",
    contain: "pbr_double",
    reinhard: "pbr_double",
    linear: "pbr_double",
    grading: "mixer",
    edges: "edges",
    bloom: "composite",
    combined: "edges",
    midnight: "pbr_double",
    ember: "pbr_double",
  };
  for (const name of order)
    assert.deepEqual(presets[name].nodes.at(-1).inputs.color, {
      node: finalSource[name],
      socket: "color",
    });
});
