export const GRAPH_ID = "authored_gpu_culling";
export const CATALOG_VERSION = 5;
const exact = (type) => ({ kind: "exact", types: [type] });
const i = (type, required = true, authoringType) => ({
  accepted: typeof type === "string" ? exact(type) : type,
  required,
  ...(authoringType ? { authoringType } : {}),
});
const o = (type) => ({ type });
const texture = {
  residency: "transient",
  texture: {
    dimension: "d2",
    format: "rgba16_float",
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
};
export const semanticCatalog = Object.freeze({
  mesh: {
    version: 1,
    execution: "source",
    inputs: {},
    outputs: {
      mesh: o("mesh_data"),
      localAabbs: o("local_aabb_buffer"),
      isVisible: {
        ...o("boolean_flag_buffer"),
        authoringType: "visibility_flag_buffer",
      },
      pipelineIndices: o("pipeline_index_stream"),
    },
    parameters: {},
  },
  texture: {
    version: 1,
    execution: "source",
    inputs: {},
    outputs: { texture: o("texture") },
    parameters: {
      residency: "transient",
      format: "rgba16_float",
      dimension: "d2",
      extentMode: "surface_relative",
      absoluteWidth: 1,
      absoluteHeight: 1,
      relativeWidthNumerator: 1,
      relativeWidthDenominator: 1,
      relativeHeightNumerator: 1,
      relativeHeightDenominator: 1,
      depthOrArrayLayers: 1,
      mipLevelCount: 1,
      sampleCount: "1",
      viewFormat: "none",
    },
  },
  frustum_cull: {
    version: 1,
    execution: "compute",
    inputs: {
      mesh: i("mesh_data"),
      localAabbs: i("local_aabb_buffer"),
    },
    outputs: {
      isFrustumCulled: {
        ...o("boolean_flag_buffer"),
        authoringType: "frustum_flag_buffer",
      },
    },
    parameters: { cameraSelection: "active" },
  },
  mesh_query: {
    version: 1,
    execution: "compute",
    inputs: {
      mesh: i("mesh_data"),
      isVisible: i("boolean_flag_buffer", false, "visibility_flag_buffer"),
      isFrustumCulled: i("boolean_flag_buffer", false, "frustum_flag_buffer"),
    },
    outputs: { draws: o("draw_stream") },
    parameters: {
      visiblePredicate: "required_true",
      frustumCulledPredicate: "required_false",
    },
  },
  pipeline_registry: {
    version: 1,
    execution: "cpu_preparation",
    inputs: { pipelineIndices: i("pipeline_index_stream") },
    outputs: { activation: o("pipeline_activation") },
    parameters: {},
  },
  pipeline: {
    version: 1,
    execution: "render",
    inputs: {
      mesh: i("mesh_data"),
      draws: i("draw_stream"),
      activation: i("pipeline_activation"),
      colorTarget: i("texture"),
      depthTarget: i("texture"),
    },
    outputs: { color: o("texture"), depth: o("texture") },
    parameters: { pipeline: "gltf_standard", depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor: [0.015, 0.02, 0.03, 1] },
  },
  fullscreen_copy: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: {},
  },
  tone_map: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: { exposure: 1 },
  },
  color_balance: {
    version: 1,
    execution: "render",
    inputs: { source: i("texture"), colorTarget: i("texture") },
    outputs: { color: o("texture") },
    parameters: {
      mode: "lift_gamma_gain", factor: 1,
      lift: 0, liftColor: [1, 1, 1, 1], gamma: 1, gammaColor: [1, 1, 1, 1], gain: 1, gainColor: [1, 1, 1, 1],
      offset: 0, offsetColor: [1, 1, 1, 1], power: 1, powerColor: [1, 1, 1, 1], slope: 1, slopeColor: [1, 1, 1, 1],
    },
  },
  exposure_contrast: {
    version: 1, execution: "render",
    inputs: { source: i("texture"), colorTarget: i("texture") }, outputs: { color: o("texture") },
    parameters: { exposureStops: 0, contrast: 1, pivot: 0.18, factor: 1 },
  },
  saturation: {
    version: 1, execution: "render",
    inputs: { source: i("texture"), colorTarget: i("texture") }, outputs: { color: o("texture") },
    parameters: { saturation: 1, factor: 1 },
  },
  channel_mixer: {
    version: 1, execution: "render",
    inputs: { source: i("texture"), colorTarget: i("texture") }, outputs: { color: o("texture") },
    parameters: { redOutput: [1, 0, 0], greenOutput: [0, 1, 0], blueOutput: [0, 0, 1], factor: 1 },
  },
  bloom_extract: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: { threshold: 1, knee: 0.5 },
  },
  bloom_blur: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: { direction: [1, 0], radius: 1 },
  },
  bloom_composite: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      bloom: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: { intensity: 1 },
  },
  luminance_edge: {
    version: 1,
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i("texture"),
    },
    outputs: { color: o("texture") },
    parameters: { strength: 2 },
  },
  frame_out: {
    version: 1,
    execution: "frame",
    inputs: { color: i("texture") },
    outputs: {},
    parameters: {},
  },
});
const socketColors = [
  "#d17c7c",
  "#d19e7c",
  "#d1c77c",
  "#9ed17c",
  "#7cd1a5",
  "#7ccbd1",
  "#7c98d1",
  "#a27cd1",
  "#d17cb8",
];
export const socketTypes = Object.fromEntries(
  [
    "texture",
    "mesh_data",
    "local_aabb_buffer",
    "boolean_flag_buffer",
    "pipeline_index_stream",
    "draw_stream",
    "pipeline_activation",
    "visibility_flag_buffer",
    "frustum_flag_buffer",
  ].map((type, index) => [
    type,
    {
      title: type.replaceAll("_", " "),
      color: socketColors[index % socketColors.length],
      acceptsFrom: [type],
    },
  ]),
);
socketTypes.boolean_flag_buffer.acceptsFrom = [
  "boolean_flag_buffer",
  "visibility_flag_buffer",
  "frustum_flag_buffer",
];
export const theme = {
  background: "#151820",
  grid: "#292e3a",
  frame: "#30343a80",
  frameHeader: "#59616c",
  body: "#292e39",
  control: "#24272b",
  controlFill: "#4775b8",
  controlEditing: "#181a1d",
  textSelection: "#4775b8",
  outline: "#0b0d12",
  text: "#edf1f7",
  muted: "#969eaa",
  shadow: "#00000088",
  nodeSelected: "#ff9f43",
  nodeActive: "#ffffff",
  unknownHeader: "#555b64",
  unknownSocket: "#999999",
  linkMuted: "#d94b4b",
  knifeMuted: "#e85b5b",
  emphasis: "#ffffff",
  focus: "#f5a623",
  editOutline: "#666a70",
  resize: "#8b8e95",
  muteOverlay: "#14141459",
  boxSelectionFill: "#f5a6231f",
  checkerLight: "#aaaaaa",
  checkerDark: "#777777",
  widgetBorder: "#111216",
  rampBorder: "#111111",
  resourceBackground: "#202228",
};
export const styles = {
  source: { header: "#3977a8" },
  compute: { header: "#725a9b" },
  cpu_preparation: { header: "#8a6d3b" },
  render: { header: "#426b43" },
  frame: { header: "#a75d37" },
};
const socket = (title, direction, type, value = null) => ({
  title,
  direction,
  type,
  maxIncomingLinks: direction === "input" ? 1 : 0,
  visible: true,
  value,
  showValue: value !== null,
});
const tagged = (kind, value) => ({ kind, value: structuredClone(value) });
const number = (value, minimum, maximum) => ({
  type: "number",
  default: tagged("number", value),
  minimum,
  maximum,
});
const enumeration = (value, values) => ({
  type: "string",
  default: tagged("string", value),
  enum: values,
});
const string = (value) => ({ type: "string", default: tagged("string", value) });
const boolean = (value) => ({
  type: "boolean",
  default: tagged("boolean", value),
});
const color = (value, minimum = 0, maximum = 1) => ({
  type: "color",
  default: tagged("color", value),
  minimum,
  maximum,
});
const vector = (value, minimum, maximum) => ({ type: "vector", default: tagged("vector", value), minimum, maximum });
const json = (value) => ({ type: "json", default: tagged("json", value) });
const parameterSchemas = {
  texture: {
    residency: enumeration("transient", ["transient", "persistent"]),
    format: enumeration("rgba16_float", [
      "rgba8_unorm",
      "rgba8_unorm_srgb",
      "bgra8_unorm",
      "bgra8_unorm_srgb",
      "rgba16_float",
      "r32_float",
      "depth32_float",
    ]),
    dimension: enumeration("d2", ["d1", "d2", "d3"]),
    extentMode: enumeration("surface_relative", [
      "surface_relative",
      "absolute",
    ]),
    absoluteWidth: { ...number(1, 1, 0xffffffff), integer: true },
    absoluteHeight: { ...number(1, 1, 0xffffffff), integer: true },
    relativeWidthNumerator: { ...number(1, 1, 0xffffffff), integer: true },
    relativeWidthDenominator: { ...number(1, 1, 0xffffffff), integer: true },
    relativeHeightNumerator: { ...number(1, 1, 0xffffffff), integer: true },
    relativeHeightDenominator: { ...number(1, 1, 0xffffffff), integer: true },
    depthOrArrayLayers: { ...number(1, 1, 0xffffffff), integer: true },
    mipLevelCount: { ...number(1, 1, 0xffffffff), integer: true },
    sampleCount: enumeration("1", ["1", "4"]),
    viewFormat: enumeration("none", [
      "none",
      "rgba8_unorm",
      "rgba8_unorm_srgb",
      "bgra8_unorm",
      "bgra8_unorm_srgb",
      "rgba16_float",
      "r32_float",
      "depth32_float",
    ]),
  },
  mesh: {},
  frustum_cull: { cameraSelection: enumeration("active", ["active"]) },
  mesh_query: {
    visiblePredicate: enumeration("required_true", [
      "any",
      "required_true",
      "required_false",
    ]),
    frustumCulledPredicate: enumeration("required_false", [
      "any",
      "required_true",
      "required_false",
    ]),
  },
  pipeline_registry: {},
  pipeline: {
    pipeline: string("gltf_standard"),
    depthCompare: enumeration("less_equal", [
      "never",
      "less",
      "equal",
      "less_equal",
      "greater",
      "not_equal",
      "greater_equal",
      "always",
    ]),
    depthWriteEnabled: boolean(true),
    clearDepth: number(1, 0, 1),
    clearColor: color([0.015, 0.02, 0.03, 1]),
  },
  fullscreen_copy: {},
  tone_map: { exposure: number(1, 0, 32) },
  color_balance: {
    mode: enumeration("lift_gamma_gain", ["lift_gamma_gain", "offset_power_slope"]), factor: number(1, 0, 1),
    lift: number(0, -1, 1), liftColor: color([1, 1, 1, 1], 0, 4), gamma: number(1, 0.01, 4), gammaColor: color([1, 1, 1, 1], 0, 4), gain: number(1, 0, 4), gainColor: color([1, 1, 1, 1], 0, 4),
    offset: number(0, -1, 1), offsetColor: color([1, 1, 1, 1], 0, 2), power: number(1, 0.01, 4), powerColor: color([1, 1, 1, 1], 0, 4), slope: number(1, 0, 4), slopeColor: color([1, 1, 1, 1], 0, 4),
  },
  exposure_contrast: { exposureStops: number(0, -10, 10), contrast: number(1, 0.01, 4), pivot: number(0.18, 0.001, 4), factor: number(1, 0, 1) },
  saturation: { saturation: number(1, 0, 4), factor: number(1, 0, 1) },
  channel_mixer: { redOutput: vector([1, 0, 0], -2, 2), greenOutput: vector([0, 1, 0], -2, 2), blueOutput: vector([0, 0, 1], -2, 2), factor: number(1, 0, 1) },
  bloom_extract: { threshold: number(1, 0, 64), knee: number(0.5, 0, 1) },
  bloom_blur: {
    direction: enumeration("horizontal", ["horizontal", "vertical"]),
    radius: number(1, 1, 16),
  },
  bloom_composite: { intensity: number(1, 0, 16) },
  luminance_edge: { strength: number(2, 0, 16) },
  frame_out: {},
};
export const nodeDefinitions = Object.fromEntries(
  Object.entries(semanticCatalog).map(([key, c]) => {
    const sockets = {
        ...Object.fromEntries(
          Object.entries(c.inputs).map(([n, v]) => [
            n,
            socket(
              n,
              "input",
              v.authoringType ?? v.accepted.types[0],
              key === "mesh_query" && n === "isVisible"
                ? boolean(true)
                : key === "mesh_query" && n === "isFrustumCulled"
                  ? boolean(false)
                  : null,
            ),
          ]),
        ),
        ...Object.fromEntries(
          Object.entries(c.outputs).map(([n, v]) => [
            n,
            socket(n, "output", v.authoringType ?? v.type),
          ]),
        ),
      },
      parameters = parameterSchemas[key];
    if (
      !parameters ||
      Object.keys(parameters).length !== Object.keys(c.parameters).length ||
      !Object.keys(c.parameters).every((name) =>
        Object.hasOwn(parameters, name),
      )
    )
      throw new Error(`parameter schema mismatch for ${key}`);
    return [
      key,
      {
        version: c.version,
        title: key.replaceAll("_", " "),
        behavior: "standard",
        style: c.execution,
        parameters,
        sockets,
        ui: [
          ...Object.keys(parameters).map((parameter) => ({
            kind: "parameter",
            parameter,
            ...(key === "frustum_cull" && parameter === "cameraSelection"
              ? { title: "Camera" }
              : {}),
          })),
          ...Object.keys(sockets).map((socket) => ({ kind: "socket", socket })),
        ],
        muteBypass: [],
        migrations: [],
      },
    ];
  }),
);
nodeDefinitions.color_balance.ui = [
  { kind: "parameter", parameter: "mode" },
  { kind: "widget", widget: "grading-wheels", bindings: [
    { title: "Lift", scalar: "lift", color: "liftColor" }, { title: "Gamma", scalar: "gamma", color: "gammaColor" }, { title: "Gain", scalar: "gain", color: "gainColor" },
  ], visibleWhen: { parameter: "mode", equals: "lift_gamma_gain" } },
  { kind: "widget", widget: "grading-wheels", bindings: [
    { title: "Offset", scalar: "offset", color: "offsetColor" }, { title: "Power", scalar: "power", color: "powerColor" }, { title: "Slope", scalar: "slope", color: "slopeColor" },
  ], visibleWhen: { parameter: "mode", equals: "offset_power_slope" } },
  { kind: "parameter", parameter: "factor" },
  { kind: "socket", socket: "source" }, { kind: "socket", socket: "colorTarget" }, { kind: "socket", socket: "color" },
];
export const fxNodeComposition = Object.freeze({
  schemaVersion: 2,
  id: "yawn.render-graph",
  version: CATALOG_VERSION,
  compatibility: { wildcardInputTypes: [] },
  socketTypes,
  nodeStyles: styles,
  resources: {},
  theme,
  nodes: nodeDefinitions,
});
export const descriptors = Object.fromEntries(
  Object.entries(semanticCatalog).map(([key, c]) => [
    key,
    {
      version: c.version,
      inputs: c.inputs,
      outputs: c.outputs,
      parameters: c.parameters,
    },
  ]),
);
