import { descriptors } from "./catalog.js";

const input = (node, socket) => [{ node, socket }];
const node = (id, key, parameters = {}, inputs = {}) => ({
  id, state: "enabled", executor: { key, version: descriptors[key].version }, parameters, inputs,
});
const texture = (format, scale = 1, heightScale = scale, sampleCount = 1) => ({
  texture: {
    dimension: "d2", format,
    extent: { kind: "surface_relative", width: { numerator: 1, denominator: scale }, height: { numerator: 1, denominator: heightScale }, depthOrArrayLayers: 1 },
    mipLevelCount: 1, sampleCount, viewFormats: [],
  },
  residency: "transient",
});
const frameOut = (hdr, options = {}) => ({ surfaceFormat: "preferred", hdrEnabled: hdr, toneMapper: "aces", exposureStops: 0, outputTransfer: "srgb", scaleMode: "stretch", filter: "linear", backgroundColor: [0, 0, 0, 1], ...options });
const predicates = (withCulling = false) => {
  const result = [
    node("type_words", "separate_u32x16", { valueDefault: Array(16).fill(0) }, { value: input("mesh", "type") }),
    node("type_bits", "separate_u32_bits", { valueDefault: 0 }, { value: input("type_words", "word0") }),
    node("ground_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit1")] }),
    node("not_double", "not", { operandDefault: false }, { operand: input("type_bits", "bit3") }),
    node("standard_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit2"), ...input("not_double", "value")] }),
    node("double_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit3")] }),
  ];
  if (!withCulling) return { nodes: result, classes: { ground: "ground_class", pbr: "standard_class", pbr_double: "double_class" } };
  result.push(
    node("cull", "frustum_cull", { camera: "active" }, { mesh: input("mesh", "mesh"), localAabb: input("mesh", "localAabb") }),
    node("not_culled", "not", { operandDefault: false }, { operand: input("cull", "isFrustumCulled") }),
  );
  for (const id of ["ground_class", "standard_class", "double_class"])
    result.find((item) => item.id === id).inputs.inputs.push(...input("not_culled", "value"));
  return { nodes: result, classes: { ground: "ground_class", pbr: "standard_class", pbr_double: "double_class" } };
};
const scene = (colorTarget, clearColor = [0.015, 0.02, 0.03, 1], heightScale = 1, withCulling = false) => {
  const classification = predicates(withCulling);
  const explicit = colorTarget || heightScale !== 1;
  const target = colorTarget || "hdr";
  return [
  ...(explicit && !colorTarget ? [node("hdr", "texture", texture("rgba16_float", 1, heightScale))] : []),
  node("mesh", "mesh"),
  ...classification.nodes,
  node("ground", "pipeline", { pipeline: "ground_plane", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor, predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input(classification.classes.ground, "value"), ...(explicit ? { colorTarget: input(target, "texture") } : {}) }),
  node("pbr", "pipeline", { pipeline: "gltf_standard", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor, predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input(classification.classes.pbr, "value"), colorTarget: input("ground", "color"), depthTarget: input("ground", "depth") }),
  node("pbr_double", "pipeline", { pipeline: "gltf_standard_double_sided", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor, predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input(classification.classes.pbr_double, "value"), colorTarget: input("pbr", "color"), depthTarget: input("pbr", "depth") }),
];
};
const graph = (graphId, nodes) => Object.freeze({ schemaVersion: 3, graphId, revision: 2, nodes });
const direct = (graphId, clearColor) => graph(graphId, [
  node("ldr", "texture", texture("rgba8_unorm")),
  ...scene("ldr", clearColor),
  node("frame_out", "frame_out", frameOut(false), { color: input("pbr_double", "color") }),
]);
export const midnight = direct("preset_midnight", [0.015, 0.06, 0.18, 1]);
export const ember = direct("preset_ember", [0.18, 0.035, 0.012, 1]);
export const hdr = graph("preset_hdr_fullscreen", [
  ...scene(),
  node("frame_out", "frame_out", frameOut(true), { color: input("pbr_double", "color") }),
]);
export const msaa = graph("preset_msaa", [
  node("msaa_hdr", "texture", texture("rgba16_float", 1, 1, 4)),
  ...scene("msaa_hdr"),
  node("frame_out", "frame_out", frameOut(true), { color: input("pbr_double", "color") }),
]);
export const culling = graph("preset_gpu_culling", (() => {
  return [...scene(undefined, undefined, 1, true), node("frame_out", "frame_out", frameOut(true), { color: input("pbr_double", "color") })];
})());
const postPreset = (graphId, kind) => {
  const nodes = [...scene()];
  let source = "pbr_double";
  if (kind === "edges") {
    nodes.splice(0, 0, node("edge_hdr", "texture", texture("rgba16_float")));
    nodes.push(node("edges", "luminance_edge", { strength: 2 }, { source: input(source, "color"), colorTarget: input("edge_hdr", "texture") }));
    source = "edges";
  }
  if (kind === "bloom" || kind === "combined") {
    nodes.splice(0, 0,
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
      nodes.splice(0, 0, node("edge_hdr", "texture", texture("rgba16_float")));
      nodes.push(node("edges", "luminance_edge", { strength: 2 }, { source: input(source, "color"), colorTarget: input("edge_hdr", "texture") }));
      source = "edges";
    }
  }
  nodes.push(node("frame_out", "frame_out", frameOut(true), { color: input(source, "color") }));
  return graph(graphId, nodes);
};
export const tone = postPreset("preset_tone", "tone");
const displayPreset = (id, parameters, heightScale = 1) => graph(id, [
  ...scene(undefined, undefined, heightScale),
  node("frame_out", "frame_out", parameters, { color: input("pbr_double", "color") }),
]);
export const contain = displayPreset("preset_contain", frameOut(false, { scaleMode: "contain", filter: "nearest", backgroundColor: [0.18, 0.18, 0.18, 0.25] }), 2);
export const reinhard = displayPreset("preset_reinhard", frameOut(true, { toneMapper: "reinhard", exposureStops: 2, scaleMode: "cover", filter: "nearest" }), 2);
export const linear = displayPreset("preset_linear", frameOut(true, { toneMapper: "none", exposureStops: 2, outputTransfer: "linear" }));
export const edges = postPreset("preset_edges", "edges");
export const bloom = postPreset("preset_bloom", "bloom");
export const combined = postPreset("preset_combined", "combined");
export const grading = graph("preset_grading", [
  node("balance_hdr", "texture", texture("rgba16_float")), node("exposure_hdr", "texture", texture("rgba16_float")), node("saturation_hdr", "texture", texture("rgba16_float")), node("mixer_hdr", "texture", texture("rgba16_float")),
  ...scene(),
  node("balance", "color_balance", { mode: "lift_gamma_gain", factor: 1, lift: 0, liftColor: [1,1,1,1], gamma: 1, gammaColor: [1,1,1,1], gain: 1, gainColor: [1,1,1,1], offset: 0, offsetColor: [1,1,1,1], power: 1, powerColor: [1,1,1,1], slope: 1, slopeColor: [1,1,1,1] }, { source: input("pbr_double", "color"), colorTarget: input("balance_hdr", "texture") }),
  node("exposure", "exposure_contrast", { exposureStops: 0, contrast: 1, pivot: 0.18, factor: 1 }, { source: input("balance", "color"), colorTarget: input("exposure_hdr", "texture") }),
  node("saturation", "saturation", { saturation: 1, factor: 1 }, { source: input("exposure", "color"), colorTarget: input("saturation_hdr", "texture") }),
  node("mixer", "channel_mixer", { redOutput: [1,0,0], greenOutput: [0,1,0], blueOutput: [0,0,1], factor: 1 }, { source: input("saturation", "color"), colorTarget: input("mixer_hdr", "texture") }),
  node("frame_out", "frame_out", frameOut(true), { color: input("mixer", "color") }),
]);
export const renderGraphPresets = Object.freeze({ midnight, ember, hdr, msaa, culling, tone, contain, reinhard, linear, grading, edges, bloom, combined });
