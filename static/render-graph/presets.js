import { descriptors } from "./catalog.js";

const input = (node, socket) => ({ node, socket });
const node = (id, key, parameters = {}, inputs = {}) => ({
  id, state: "enabled", executor: { key, version: descriptors[key].version }, parameters, inputs,
});
const texture = (format, scale = 1) => ({
  texture: {
    dimension: "d2", format,
    extent: { kind: "surface_relative", width: { numerator: 1, denominator: scale }, height: { numerator: 1, denominator: scale }, depthOrArrayLayers: 1 },
    mipLevelCount: 1, sampleCount: 1, viewFormats: [],
  },
  residency: "transient",
});
const scene = (colorTarget, clearColor = [0.015, 0.02, 0.03, 1]) => [
  node("hdr", "texture", texture("rgba16_float")),
  node("depth", "texture", texture("depth32_float")),
  node("mesh", "mesh"),
  node("query", "mesh_query", { visiblePredicate: "required_true", visibleDefault: true, frustumCulledPredicate: "any", frustumCulledDefault: false }, { mesh: input("mesh", "mesh"), isVisible: input("mesh", "isVisible") }),
  node("registry", "pipeline_registry", {}, { pipelineIndices: input("mesh", "pipelineIndices") }),
  node("ground", "pipeline", { pipeline: "ground_plane", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor }, { mesh: input("mesh", "mesh"), draws: input("query", "draws"), activation: input("registry", "activation"), colorTarget: input(colorTarget, "texture"), depthTarget: input("depth", "texture") }),
  node("pbr", "pipeline", { pipeline: "gltf_standard", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor }, { mesh: input("mesh", "mesh"), draws: input("query", "draws"), activation: input("registry", "activation"), colorTarget: input("ground", "color"), depthTarget: input("ground", "depth") }),
  node("pbr_double", "pipeline", { pipeline: "gltf_standard_double_sided", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor }, { mesh: input("mesh", "mesh"), draws: input("query", "draws"), activation: input("registry", "activation"), colorTarget: input("pbr", "color"), depthTarget: input("pbr", "depth") }),
];
const graph = (graphId, nodes) => Object.freeze({ schemaVersion: 2, graphId, revision: 1, nodes });
const direct = (graphId, clearColor) => graph(graphId, [
  node("ldr", "texture", texture("rgba8_unorm")),
  ...scene("ldr", clearColor).filter((item) => item.id !== "hdr"),
  node("frame_out", "frame_out", {}, { color: input("pbr_double", "color") }),
]);
export const midnight = direct("preset_midnight", [0.015, 0.06, 0.18, 1]);
export const ember = direct("preset_ember", [0.18, 0.035, 0.012, 1]);
export const hdr = graph("preset_hdr_fullscreen", [
  ...scene("hdr"),
  node("frame_out", "frame_out", {}, { color: input("pbr_double", "color") }),
]);
export const culling = graph("preset_gpu_culling", (() => {
  const nodes = structuredClone(hdr.nodes);
  nodes.splice(3, 0, node("cull", "frustum_cull", { camera: "active" }, { mesh: input("mesh", "mesh"), localAabbs: input("mesh", "localAabbs") }));
  const query = nodes.find((item) => item.id === "query");
  query.parameters.frustumCulledPredicate = "required_false";
  query.inputs.isFrustumCulled = input("cull", "isFrustumCulled");
  return nodes;
})());
const postPreset = (graphId, kind) => {
  const nodes = [node("ldr", "texture", texture("rgba8_unorm")), ...scene("hdr")];
  let source = "pbr_double";
  if (kind === "edges") {
    nodes.splice(1, 0, node("edge_hdr", "texture", texture("rgba16_float")));
    nodes.push(node("edges", "luminance_edge", { strength: 2 }, { source: input(source, "color"), colorTarget: input("edge_hdr", "texture") }));
    source = "edges";
  }
  if (kind === "bloom" || kind === "combined") {
    nodes.splice(1, 0,
      node("half_a", "texture", texture("rgba16_float", 2)), node("half_b", "texture", texture("rgba16_float", 2)),
      node("half_c", "texture", texture("rgba16_float", 2)), node("composite_hdr", "texture", texture("rgba16_float")));
    nodes.push(
      node("extract", "bloom_extract", { threshold: 1, knee: 0.5 }, { source: input("pbr_double", "color"), colorTarget: input("half_a", "texture") }),
      node("blur_h", "bloom_blur", { direction: [1, 0], radius: 1 }, { source: input("extract", "color"), colorTarget: input("half_b", "texture") }),
      node("blur_v", "bloom_blur", { direction: [0, 1], radius: 1 }, { source: input("blur_h", "color"), colorTarget: input("half_c", "texture") }),
      node("composite", "bloom_composite", { intensity: 0.8 }, { source: input("pbr_double", "color"), bloom: input("blur_v", "color"), colorTarget: input("composite_hdr", "texture") }),
    );
    source = "composite";
    if (kind === "combined") {
      nodes.splice(1, 0, node("edge_hdr", "texture", texture("rgba16_float")));
      nodes.push(node("edges", "luminance_edge", { strength: 2 }, { source: input(source, "color"), colorTarget: input("edge_hdr", "texture") }));
      source = "edges";
    }
  }
  nodes.push(node("tone", "tone_map", { exposure: 1 }, { source: input(source, "color"), colorTarget: input("ldr", "texture") }));
  nodes.push(node("frame_out", "frame_out", {}, { color: input("tone", "color") }));
  return graph(graphId, nodes);
};
export const tone = postPreset("preset_tone", "tone");
export const edges = postPreset("preset_edges", "edges");
export const bloom = postPreset("preset_bloom", "bloom");
export const combined = postPreset("preset_combined", "combined");
export const grading = graph("preset_grading", [
  node("balance_hdr", "texture", texture("rgba16_float")), node("exposure_hdr", "texture", texture("rgba16_float")), node("saturation_hdr", "texture", texture("rgba16_float")), node("mixer_hdr", "texture", texture("rgba16_float")), node("ldr", "texture", texture("rgba8_unorm")),
  ...scene("hdr"),
  node("balance", "color_balance", { mode: "lift_gamma_gain", factor: 1, lift: 0, liftColor: [1,1,1,1], gamma: 1, gammaColor: [1,1,1,1], gain: 1, gainColor: [1,1,1,1], offset: 0, offsetColor: [1,1,1,1], power: 1, powerColor: [1,1,1,1], slope: 1, slopeColor: [1,1,1,1] }, { source: input("pbr_double", "color"), colorTarget: input("balance_hdr", "texture") }),
  node("exposure", "exposure_contrast", { exposureStops: 0, contrast: 1, pivot: 0.18, factor: 1 }, { source: input("balance", "color"), colorTarget: input("exposure_hdr", "texture") }),
  node("saturation", "saturation", { saturation: 1, factor: 1 }, { source: input("exposure", "color"), colorTarget: input("saturation_hdr", "texture") }),
  node("mixer", "channel_mixer", { redOutput: [1,0,0], greenOutput: [0,1,0], blueOutput: [0,0,1], factor: 1 }, { source: input("saturation", "color"), colorTarget: input("mixer_hdr", "texture") }),
  node("tone", "tone_map", { exposure: 1 }, { source: input("mixer", "color"), colorTarget: input("ldr", "texture") }), node("frame_out", "frame_out", {}, { color: input("tone", "color") }),
]);
export const renderGraphPresets = Object.freeze({ midnight, ember, hdr, culling, tone, grading, edges, bloom, combined });
