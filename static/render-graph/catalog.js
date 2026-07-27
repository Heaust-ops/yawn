export const GRAPH_ID = "authored_gpu_culling";
export const CATALOG_VERSION = 2;
const exact = (type) => ({ kind: "exact", types: [type] });
const oneOf = (...types) => ({ kind: "one_of", types });
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
  surface_target: {
    execution: "source",
    inputs: {},
    outputs: { surface: o("surface_target") },
    parameters: {},
  },
  texture_spec: {
    execution: "source",
    inputs: {},
    outputs: { spec: o("texture_spec") },
    parameters: structuredClone(texture),
  },
  scene_table: {
    execution: "source",
    inputs: {},
    outputs: { scene: o("scene_table") },
    parameters: {},
  },
  local_aabb_buffer: {
    execution: "source",
    inputs: { scene: i("scene_table") },
    outputs: { localAabbs: o("local_aabb_buffer") },
    parameters: {},
  },
  camera_frustum: {
    execution: "source",
    inputs: {},
    outputs: { frustum: o("camera_frustum") },
    parameters: {},
  },
  visibility_flags: {
    execution: "source",
    inputs: { scene: i("scene_table") },
    outputs: {
      flags: {
        ...o("boolean_flag_buffer"),
        authoringType: "visibility_flag_buffer",
      },
    },
    parameters: {},
  },
  frustum_cull: {
    execution: "compute",
    inputs: {
      scene: i("scene_table"),
      localAabbs: i("local_aabb_buffer"),
      frustum: i("camera_frustum"),
    },
    outputs: {
      flags: {
        ...o("boolean_flag_buffer"),
        authoringType: "frustum_flag_buffer",
      },
    },
    parameters: {},
  },
  mesh_query: {
    execution: "compute",
    inputs: {
      scene: i("scene_table"),
      isVisible: i("boolean_flag_buffer", false, "visibility_flag_buffer"),
      isFrustumCulled: i("boolean_flag_buffer", false, "frustum_flag_buffer"),
    },
    outputs: { draws: o("draw_stream") },
    parameters: {
      filters: [
        { flag: "isVisible", predicate: "required_true" },
        { flag: "isFrustumCulled", predicate: "required_false" },
      ],
    },
  },
  depth_stencil_config: {
    execution: "source",
    inputs: {},
    outputs: { config: o("depth_stencil_config") },
    parameters: {
      depthCompare: "less_equal",
      depthWriteEnabled: true,
      clearDepth: 1,
    },
  },
  legacy_forward: {
    execution: "render",
    inputs: {
      scene: i("scene_table"),
      draws: i("draw_stream"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
      depthTarget: i(oneOf("texture_spec", "texture")),
      depthStencil: i("depth_stencil_config"),
    },
    outputs: { color: o("texture"), depth: o("texture") },
    parameters: { clearColor: [0.015, 0.02, 0.03, 1] },
  },
  fullscreen_copy: {
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: {},
  },
  tone_map: {
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: { exposure: 1 },
  },
  bloom_extract: {
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: { threshold: 1, knee: 0.5 },
  },
  bloom_blur: {
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: { direction: [1, 0], radius: 1 },
  },
  bloom_composite: {
    execution: "render",
    inputs: {
      source: i("texture"),
      bloom: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: { intensity: 1 },
  },
  luminance_edge: {
    execution: "render",
    inputs: {
      source: i("texture"),
      colorTarget: i(oneOf("surface_target", "texture_spec", "texture")),
    },
    outputs: { color: o("texture") },
    parameters: { strength: 2 },
  },
  present: {
    execution: "present",
    inputs: { surface: i("texture") },
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
    "surface_target",
    "texture_spec",
    "texture",
    "scene_table",
    "local_aabb_buffer",
    "camera_frustum",
    "boolean_flag_buffer",
    "draw_stream",
    "depth_stencil_config",
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
socketTypes.surface_target.acceptsFrom = [
  "surface_target",
  "texture_spec",
  "texture",
];
socketTypes.texture_spec.acceptsFrom = ["texture_spec", "texture"];
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
  render: { header: "#426b43" },
  present: { header: "#a75d37" },
};
const socket = (title, direction, type) => ({
  title,
  direction,
  type,
  maxIncomingLinks: direction === "input" ? 1 : 0,
  visible: true,
  value: null,
  showValue: false,
});
const parameterSchema = (value) =>
  typeof value === "number"
    ? { type: "number", default: { kind: "number", value } }
    : typeof value === "string"
      ? { type: "string", default: { kind: "string", value } }
      : typeof value === "boolean"
        ? { type: "boolean", default: { kind: "boolean", value } }
        : { type: "json", default: { kind: "json", value } };
export const nodeDefinitions = Object.fromEntries(
  Object.entries(semanticCatalog).map(([key, c]) => {
    const sockets = {
        ...Object.fromEntries(
          Object.entries(c.inputs).map(([n, v]) => [
            n,
            socket(n, "input", v.authoringType ?? v.accepted.types[0]),
          ]),
        ),
        ...Object.fromEntries(
          Object.entries(c.outputs).map(([n, v]) => [
            n,
            socket(n, "output", v.authoringType ?? v.type),
          ]),
        ),
      },
      parameters = Object.fromEntries(
        Object.entries(c.parameters).map(([name, value]) => [
          name,
          parameterSchema(value),
        ]),
      );
    return [
      key,
      {
        version: 1,
        title: key.replaceAll("_", " "),
        behavior: "standard",
        style: c.execution,
        parameters,
        sockets,
        ui: [
          ...Object.keys(parameters).map((parameter) => ({
            kind: "parameter",
            parameter,
          })),
          ...Object.keys(sockets).map((socket) => ({ kind: "socket", socket })),
        ],
        muteBypass: [],
        migrations: [],
      },
    ];
  }),
);
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
      version: 1,
      inputs: c.inputs,
      outputs: c.outputs,
      parameters: c.parameters,
    },
  ]),
);
