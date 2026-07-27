const input = (node, socket) => ({ node, socket });
const node = (id, key, parameters = {}, inputs = {}) => ({
  id,
  state: "enabled",
  executor: { key, version: 1 },
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
const direct = (graphId, clearColor) => Object.freeze({
  schemaVersion: 2,
  graphId,
  revision: 1,
  nodes: [
    node("surface", "surface_target"),
    node("depth", "texture_spec", texture("depth32_float")),
    node("scene", "scene_table"),
    node("visible", "visibility_flags", {}, { scene: input("scene", "scene") }),
    node("query", "mesh_query", {
      filters: [
        { flag: "isVisible", predicate: "required_true" },
        { flag: "isFrustumCulled", predicate: "any" },
      ],
    }, { scene: input("scene", "scene"), isVisible: input("visible", "flags") }),
    node("depth_config", "depth_stencil_config", {
      depthCompare: "less_equal",
      depthWriteEnabled: true,
      clearDepth: 1,
    }),
    node("forward", "legacy_forward", { clearColor }, {
      scene: input("scene", "scene"),
      draws: input("query", "draws"),
      colorTarget: input("surface", "surface"),
      depthTarget: input("depth", "spec"),
      depthStencil: input("depth_config", "config"),
    }),
    node("present", "present", {}, { surface: input("forward", "color") }),
  ],
});
export const midnight = direct("preset_midnight", [0.015, 0.06, 0.18, 1]);
export const ember = direct("preset_ember", [0.18, 0.035, 0.012, 1]);
export const hdr = Object.freeze({
  schemaVersion: 2,
  graphId: "preset_hdr_fullscreen",
  revision: 1,
  nodes: [
    node("surface", "surface_target"),
    node("hdr", "texture_spec", texture("rgba16_float")),
    node("depth", "texture_spec", texture("depth32_float")),
    node("scene", "scene_table"),
    node("visible", "visibility_flags", {}, { scene: input("scene", "scene") }),
    node(
      "query",
      "mesh_query",
      {
        filters: [
          { flag: "isVisible", predicate: "required_true" },
          { flag: "isFrustumCulled", predicate: "any" },
        ],
      },
      { scene: input("scene", "scene"), isVisible: input("visible", "flags") },
    ),
    node("depth_config", "depth_stencil_config", {
      depthCompare: "less_equal",
      depthWriteEnabled: true,
      clearDepth: 1,
    }),
    node(
      "forward",
      "legacy_forward",
      { clearColor: [0.015, 0.02, 0.03, 1] },
      {
        scene: input("scene", "scene"),
        draws: input("query", "draws"),
        colorTarget: input("hdr", "spec"),
        depthTarget: input("depth", "spec"),
        depthStencil: input("depth_config", "config"),
      },
    ),
    node(
      "copy",
      "fullscreen_copy",
      {},
      {
        source: input("forward", "color"),
        colorTarget: input("surface", "surface"),
      },
    ),
    node("present", "present", {}, { surface: input("copy", "color") }),
  ],
});
export const culling = Object.freeze((() => {
  const graph = structuredClone(hdr);
  graph.graphId = "preset_gpu_culling";
  graph.nodes.splice(
    5,
    0,
    node("aabbs", "local_aabb_buffer", {}, { scene: input("scene", "scene") }),
    node("frustum", "camera_frustum"),
    node("cull", "frustum_cull", {}, {
      scene: input("scene", "scene"),
      localAabbs: input("aabbs", "localAabbs"),
      frustum: input("frustum", "frustum"),
    }),
  );
  const query = graph.nodes.find((x) => x.id === "query");
  query.parameters.filters[1].predicate = "required_false";
  query.inputs.isFrustumCulled = input("cull", "flags");
  return graph;
})());
const postPreset = (graphId, kind) => {
  const nodes = hdr.nodes.slice(0, 8).map((x) => structuredClone(x));
  if (kind === "tone")
    nodes.push(
      node(
        "tone",
        "tone_map",
        { exposure: 1 },
        {
          source: input("forward", "color"),
          colorTarget: input("surface", "surface"),
        },
      ),
    );
  if (kind === "edges") {
    nodes.splice(1, 0, node("edge_hdr", "texture_spec", texture("rgba16_float")));
    nodes.push(
      node(
        "edges",
        "luminance_edge",
        { strength: 2 },
        {
          source: input("forward", "color"),
          colorTarget: input("edge_hdr", "spec"),
        },
      ),
    );
    nodes.push(
      node(
        "tone",
        "tone_map",
        { exposure: 1 },
        {
          source: input("edges", "color"),
          colorTarget: input("surface", "surface"),
        },
      ),
    );
  }
  if (kind === "bloom" || kind === "combined") {
    const half = {
      texture: {
        ...texture("rgba16_float").texture,
        extent: {
          kind: "surface_relative",
          width: { numerator: 1, denominator: 2 },
          height: { numerator: 1, denominator: 2 },
          depthOrArrayLayers: 1,
        },
      },
      residency: "transient",
    };
    nodes.splice(
      1,
      0,
      node("half_a", "texture_spec", structuredClone(half)),
      node("half_b", "texture_spec", structuredClone(half)),
      node("half_c", "texture_spec", structuredClone(half)),
      node("composite_hdr", "texture_spec", texture("rgba16_float")),
    );
    nodes.push(
      node(
        "extract",
        "bloom_extract",
        { threshold: 1, knee: 0.5 },
        {
          source: input("forward", "color"),
          colorTarget: input("half_a", "spec"),
        },
      ),
    );
    nodes.push(
      node(
        "blur_h",
        "bloom_blur",
        { direction: [1, 0], radius: 1 },
        {
          source: input("extract", "color"),
          colorTarget: input("half_b", "spec"),
        },
      ),
    );
    nodes.push(
      node(
        "blur_v",
        "bloom_blur",
        { direction: [0, 1], radius: 1 },
        {
          source: input("blur_h", "color"),
          colorTarget: input("half_c", "spec"),
        },
      ),
    );
    nodes.push(
      node(
        "composite",
        "bloom_composite",
        { intensity: 0.8 },
        {
          source: input("forward", "color"),
          bloom: input("blur_v", "color"),
          colorTarget: input("composite_hdr", "spec"),
        },
      ),
    );
    let toneSource = "composite";
    if (kind === "combined") {
      nodes.splice(1, 0, node("edge_hdr", "texture_spec", texture("rgba16_float")));
      nodes.push(
        node(
          "edges",
          "luminance_edge",
          { strength: 2 },
          {
            source: input("composite", "color"),
            colorTarget: input("edge_hdr", "spec"),
          },
        ),
      );
      toneSource = "edges";
    }
    nodes.push(
      node(
        "tone",
        "tone_map",
        { exposure: 1 },
        {
          source: input(toneSource, "color"),
          colorTarget: input("surface", "surface"),
        },
      ),
    );
  }
  const last = nodes.at(-1);
  nodes.push(
    node("present", "present", {}, { surface: input(last.id, "color") }),
  );
  return Object.freeze({ schemaVersion: 2, graphId, revision: 1, nodes });
};
export const tone = postPreset("preset_tone", "tone"),
  edges = postPreset("preset_edges", "edges"),
  bloom = postPreset("preset_bloom", "bloom"),
  combined = postPreset("preset_combined", "combined");
export const renderGraphPresets = Object.freeze({
  midnight,
  ember,
  hdr,
  culling,
  tone,
  edges,
  bloom,
  combined,
});
