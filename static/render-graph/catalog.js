export const GRAPH_ID = "authored_gpu_culling";
export const CATALOG_VERSION = 13;
const exact = (type) => ({ kind: "exact", types: [type] });
const i = (type, minimum = 1, authoringType, defaultPolicy = minimum ? "none" : "parameter_literal", maximum = 1) => ({
  accepted: typeof type === "string" ? exact(type) : type,
  cardinality: { minimum, maximum },
  ...(authoringType ? { authoringType } : {}),
  defaultPolicy,
});
const o = (type, semanticName) => ({ type, ...(semanticName ? { semanticName } : {}) });
const expression = (inputs, outputs) => ({
  version: 1,
  execution: "expression",
  inputs: Object.fromEntries(Object.entries(inputs).map(([name, type]) => [name, i(type, 0)])),
  outputs: Object.fromEntries(Object.entries(outputs).map(([name, type]) => [name, o(type)])),
  parameters: {},
});
const numbered = (prefix, count, type) =>
  Object.fromEntries(Array.from({ length: count }, (_, index) => [`${prefix}${index}`, type]));
const expressionCatalog = {
  and: { ...expression({ inputs: "bool" }, { value: "bool" }), version: 2, inputs: { inputs: i("bool", 0, undefined, "none", 8) } },
  or: { ...expression({ inputs: "bool" }, { value: "bool" }), version: 2, inputs: { inputs: i("bool", 0, undefined, "none", 8) } },
  not: expression({ operand: "bool" }, { value: "bool" }),
  xor: { ...expression({ inputs: "bool" }, { value: "bool" }), version: 2, inputs: { inputs: i("bool", 0, undefined, "none", 8) } },
  xnor: { ...expression({ inputs: "bool" }, { value: "bool" }), version: 2, inputs: { inputs: i("bool", 0, undefined, "none", 8) } },
  greater_than_f32: expression({ left: "f32", right: "f32" }, { value: "bool" }),
  less_than_f32: expression({ left: "f32", right: "f32" }, { value: "bool" }),
  equals_f32: expression({ left: "f32", right: "f32" }, { value: "bool" }),
  greater_than_u32: expression({ left: "u32", right: "u32" }, { value: "bool" }),
  less_than_u32: expression({ left: "u32", right: "u32" }, { value: "bool" }),
  equals_u32: expression({ left: "u32", right: "u32" }, { value: "bool" }),
  separate_vec2: expression({ vector: "vec2" }, { x: "f32", y: "f32" }),
  combine_vec2: expression({ x: "f32", y: "f32" }, { vector: "vec2" }),
  separate_vec3: expression({ vector: "vec3" }, { x: "f32", y: "f32", z: "f32" }),
  combine_vec3: expression({ x: "f32", y: "f32", z: "f32" }, { vector: "vec3" }),
  separate_vec4: expression({ vector: "vec4" }, { x: "f32", y: "f32", z: "f32", w: "f32" }),
  combine_vec4: expression({ x: "f32", y: "f32", z: "f32", w: "f32" }, { vector: "vec4" }),
  separate_mat2: expression({ matrix: "mat2" }, numbered("column", 2, "vec2")),
  combine_mat2: expression(numbered("column", 2, "vec2"), { matrix: "mat2" }),
  separate_mat3: expression({ matrix: "mat3" }, numbered("column", 3, "vec3")),
  combine_mat3: expression(numbered("column", 3, "vec3"), { matrix: "mat3" }),
  separate_mat4: expression({ matrix: "mat4" }, numbered("column", 4, "vec4")),
  combine_mat4: expression(numbered("column", 4, "vec4"), { matrix: "mat4" }),
  separate_u32x16: expression({ value: "u32x16" }, numbered("word", 16, "u32")),
  combine_u32x16: expression(numbered("word", 16, "u32"), { value: "u32x16" }),
  separate_u32_bits: expression({ value: "u32" }, numbered("bit", 32, "bool")),
  combine_u32_bits: expression(numbered("bit", 32, "bool"), { value: "u32" }),
  separate_local_aabb: expression({ value: "local_aabb" }, { min: "vec3", max: "vec3" }),
};
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
const rasterInputs = () => ({
  mesh: i("mesh_data"),
  predicate: i("bool", 0),
  "input.color": { ...i("texture", 0, undefined, "compiler_texture"), semanticName: "color" },
  "input.depth": { ...i("texture", 0, undefined, "compiler_texture"), semanticName: "depth" },
});
const rasterOutputs = () => ({ "output.color": o("texture", "color"), "output.depth": o("texture", "depth") });
const rasterParameters = () => ({ depthCompare: "less_equal", depthWriteEnabled: true, clearDepth: 1, clearColor: [0.015, 0.02, 0.03, 1] });
const raster = () => ({ version: 2, execution: "render", inputs: rasterInputs(), outputs: rasterOutputs(), parameters: rasterParameters() });
export const NODE_TITLE_OVERRIDES = Object.freeze({
  ground_plane: "Ground Plane",
  gltf_standard: "glTF Standard",
  gltf_standard_double_sided: "glTF Standard — Double-Sided",
});
export const semanticCatalog = Object.freeze({
  mesh: {
    version: 2,
    execution: "source",
    inputs: {},
    outputs: {
      mesh: o("mesh_data"),
      type: o("u32x16"),
      localAabb: o("local_aabb"),
    },
    parameters: {},
  },
  texture: {
    version: 2,
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
    version: 2,
    execution: "expression",
    inputs: {
      mesh: i("mesh_data"),
      localAabb: i("local_aabb"),
    },
    outputs: { isFrustumCulled: o("bool") },
    parameters: { cameraSelection: "active" },
  },
  ground_plane: raster(),
  gltf_standard: raster(),
  gltf_standard_double_sided: raster(),
  ...expressionCatalog,
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
    version: 3,
    execution: "frame",
    inputs: { color: i("texture") },
    outputs: {},
    parameters: { surfaceFormat: "preferred", hdrEnabled: true, toneMapper: "aces", exposureStops: 0, outputTransfer: "srgb", scaleMode: "stretch", filter: "linear", backgroundColor: [0, 0, 0, 1] },
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
    "bool", "f32", "u32", "vec2", "vec3", "vec4",
    "mat2", "mat3", "mat4", "u32x16", "local_aabb",
  ].map((type, index) => [
    type,
    {
      title: type.replaceAll("_", " "),
      color: socketColors[index % socketColors.length],
      acceptsFrom: [type],
    },
  ]),
);
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
  expression: { header: "#725a9b" },
  cpu_preparation: { header: "#8a6d3b" },
  render: { header: "#426b43" },
  frame: { header: "#a75d37" },
};
const socket = (title, direction, type, value = null, capacity = 1) => ({
  title,
  direction,
  type,
  maxIncomingLinks: direction === "input" ? capacity : 0,
  visible: true,
  value,
  showValue: value !== null,
});
const tagged = (kind, value) => ({ kind, value: structuredClone(value) });
const number = (value, minimum, maximum) => ({
  type: "number",
  default: tagged("number", value),
  ...(minimum !== undefined ? { minimum } : {}),
  ...(maximum !== undefined ? { maximum } : {}),
});
const enumeration = (value, values) => ({
  type: "string",
  default: tagged("string", value),
  enum: values,
});
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
const vector = (value, minimum, maximum) => ({
  type: "vector", default: tagged("vector", value),
  ...(minimum !== undefined ? { minimum } : {}),
  ...(maximum !== undefined ? { maximum } : {}),
});
const json = (value) => ({ type: "json", default: tagged("json", value) });
const socketDefault = (type, value) => {
  if (type === "bool") return boolean(value);
  if (type === "f32") return number(value);
  if (type === "u32") return { ...number(value, 0, 0xffffffff), integer: true };
  if (type === "vec3") return vector(value);
  return json(value);
};
const zero = (type) => {
  if (type === "bool") return false;
  if (type === "f32" || type === "u32") return 0;
  if (/^vec[234]$/.test(type)) return Array(Number(type.at(-1))).fill(0);
  if (type === "u32x16") return Array(16).fill(0);
  if (type === "local_aabb") return { min: [0, 0, 0], max: [0, 0, 0] };
  const size = Number(type.at(-1));
  return Array.from({ length: size }, (_, column) =>
    Array.from({ length: size }, (_, row) => Number(column === row)));
};
const defaultForInput = (key, name, type) => {
  if (["ground_plane", "gltf_standard", "gltf_standard_double_sided"].includes(key) && name === "predicate") return true;
  if (key === "and") return true;
  if (/^combine_mat[234]$/.test(key)) {
    const index = Number(name.replace("column", ""));
    return zero(type).map((_, row) => Number(index === row));
  }
  return zero(type);
};
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
  ground_plane: {
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
  frame_out: {
    surfaceFormat: enumeration("preferred", ["preferred", "rgba8_unorm", "bgra8_unorm", "rgba16_float"]),
    hdrEnabled: boolean(true),
    toneMapper: enumeration("aces", ["aces", "reinhard", "none"]),
    exposureStops: number(0, -10, 10),
    outputTransfer: enumeration("srgb", ["srgb", "linear"]),
    scaleMode: enumeration("stretch", ["stretch", "contain", "cover"]),
    filter: enumeration("linear", ["linear", "nearest"]),
    backgroundColor: color([0, 0, 0, 1]),
  },
};
for (const key of Object.keys(expressionCatalog)) parameterSchemas[key] = {};
parameterSchemas.gltf_standard = structuredClone(parameterSchemas.ground_plane);
parameterSchemas.gltf_standard_double_sided = structuredClone(parameterSchemas.ground_plane);
export const nodeDefinitions = Object.fromEntries(
  Object.entries(semanticCatalog).map(([key, c]) => {
    const sockets = {
        ...Object.fromEntries(
          Object.entries(c.inputs).map(([n, v]) => [
            n,
            socket(
              v.semanticName ?? n,
              "input",
              v.authoringType ?? v.accepted.types[0],
              v.defaultPolicy === "parameter_literal" ? socketDefault(v.accepted.types[0], defaultForInput(key, n, v.accepted.types[0])) : null,
              v.cardinality.maximum,
            ),
          ]),
        ),
        ...Object.fromEntries(
          Object.entries(c.outputs).map(([n, v]) => [
            n,
            socket(v.semanticName ?? n, "output", v.authoringType ?? v.type),
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
        title: NODE_TITLE_OVERRIDES[key] ?? key.replaceAll("_", " "),
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
nodeDefinitions.mesh.sockets.localAabb.title = "Local AABB";
for (const key of Object.keys(NODE_TITLE_OVERRIDES)) {
  for (const item of nodeDefinitions[key].ui) {
    if (item.parameter === "clearColor") item.title = "Initial Color";
    if (item.parameter === "clearDepth") item.title = "Initial Depth";
  }
}
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
nodeDefinitions.frame_out.ui = [
  { kind: "text", variant: "section", title: "Canvas Presentation" },
  { kind: "parameter", parameter: "surfaceFormat", title: "Surface Format" },
  { kind: "text", variant: "section", title: "Display Transform" },
  { kind: "parameter", parameter: "hdrEnabled", title: "HDR" },
  { kind: "parameter", parameter: "toneMapper", title: "Tone Mapper", visibleWhen: { parameter: "hdrEnabled", equals: true } },
  { kind: "parameter", parameter: "exposureStops", title: "Exposure", visibleWhen: { parameter: "hdrEnabled", equals: true } },
  { kind: "parameter", parameter: "outputTransfer", title: "Transfer" },
  { kind: "parameter", parameter: "scaleMode", title: "Scale" },
  { kind: "parameter", parameter: "filter" },
  { kind: "parameter", parameter: "backgroundColor", title: "Background", visibleWhen: { parameter: "scaleMode", equals: "contain" } },
  { kind: "socket", socket: "color" },
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
