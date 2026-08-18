import { defaultPipelines } from "@yawn/default-pipelines";
import { descriptors } from "@yawn/render-graph-fxnode/catalog";
import { graphFromObject } from "@yawn/render-graph-js";

// A complete graph authored like a package consumer would author it.

const input = (node, socket) => [{ node, socket }];
const node = (id, key, parameters = {}, inputs = {}) => ({
  id,
  state: "enabled",
  executor: { key, version: descriptors[key].version },
  parameters,
  inputs,
});
const texture = (format) => ({
  texture: {
    dimension: "d2",
    format,
    extent: {
      kind: "surface_relative",
      width: { numerator: 1, denominator: 1 },
      height: { numerator: 1, denominator: 1 },
      depthOrArrayLayers: 1,
    },
    mipLevelCount: 1,
    sampleCount: 1,
    viewFormats: [],
  },
  residency: "transient",
});

const nodes = [
  node("hdr", "texture", texture("rgba16_float")),
  node("scene_depth", "texture", texture("depth32_float")),
  node("mesh", "mesh"),
  node("type_words", "separate_u32x16", { valueDefault: Array(16).fill(0) }, { value: input("mesh", "type") }),
  node("type_bits", "separate_u32_bits", { valueDefault: 0 }, { value: input("type_words", "word0") }),
  node("ground_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit1")] }),
  node("not_double", "not", { operandDefault: false }, { operand: input("type_bits", "bit3") }),
  node("standard_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit2"), ...input("not_double", "value")] }),
  node("double_class", "and", {}, { inputs: [...input("type_bits", "bit0"), ...input("type_bits", "bit3")] }),
  node("cull", "frustum_cull", { camera: "active" }, { mesh: input("mesh", "mesh"), localAabb: input("mesh", "localAabb") }),
  node("not_culled", "not", { operandDefault: false }, { operand: input("cull", "isFrustumCulled") }),
];
for (const id of ["ground_class", "standard_class", "double_class"])
  nodes.find((item) => item.id === id).inputs.inputs.push(...input("not_culled", "value"));
nodes.push(
  node("ground", "ground_plane", { depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor: [0.015, 0.02, 0.03, 1], predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input("ground_class", "value"), color: input("hdr", "texture"), depth: input("scene_depth", "texture") }),
  node("pbr", "gltf_standard", { depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor: [0.015, 0.02, 0.03, 1], predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input("standard_class", "value"), color: input("hdr", "texture"), depth: input("scene_depth", "texture") }),
  node("pbr_double", "gltf_standard_double_sided", { depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor: [0.015, 0.02, 0.03, 1], predicateDefault: true }, { mesh: input("mesh", "mesh"), predicate: input("double_class", "value"), color: input("hdr", "texture"), depth: input("scene_depth", "texture") }),
  node("frame_out", "frame_out", { surfaceFormat: "preferred", hdrEnabled: true, toneMapper: "aces", exposureStops: 0, outputTransfer: "srgb", scaleMode: "stretch", filter: "linear", backgroundColor: [0, 0, 0, 1] }, { color: input("pbr_double", "color") }),
);

/** The example's JSO graph; the graph addon canonicalizes it to AST and S-expressions. */
export const culling = graphFromObject({
  id: "example_jso_scene",
  revision: 1,
  pipelines: defaultPipelines,
  nodes,
});

export const renderGraphPresets = Object.freeze({ jso: culling });
