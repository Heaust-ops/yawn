import type { FxNodeDefinition } from "@lib/composition/index.js";
export const noiseTextureNode = [
  "fxnode.shader.noise-texture",
  {
    version: 2,
    title: "Noise Texture",
    behavior: "standard",
    style: "texture",
    parameters: {
      dimensions: {
        type: "string",
        default: {
          kind: "string",
          value: "3d",
        },
        enum: ["1d", "2d", "3d", "4d"],
      },
      noiseType: {
        type: "string",
        default: {
          kind: "string",
          value: "fbm",
        },
        enum: ["fbm", "multifractal", "hybrid-multifractal", "ridged-multifractal", "hetero-terrain"],
      },
      normalize: {
        type: "boolean",
        default: {
          kind: "boolean",
          value: false,
        },
      },
    },
    sockets: {
      vector: {
        title: "Vector",
        direction: "input",
        type: "vector",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "vector",
          default: {
            kind: "vector",
            value: [0, 0, 0],
          },
        },
        showValue: false,
      },
      w: {
        title: "W",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0,
          },
        },
        showValue: true,
      },
      scale: {
        title: "Scale",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 5,
          },
          minimum: -1000,
          maximum: 1000,
        },
        showValue: true,
      },
      detail: {
        title: "Detail",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 2,
          },
          minimum: 0,
          maximum: 15,
        },
        showValue: true,
      },
      roughness: {
        title: "Roughness",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0.5,
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      lacunarity: {
        title: "Lacunarity",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 2,
          },
          minimum: 0,
          maximum: 1000,
        },
        showValue: true,
      },
      offset: {
        title: "Offset",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0,
          },
          minimum: -1000,
          maximum: 1000,
        },
        showValue: true,
      },
      gain: {
        title: "Gain",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 1,
          },
          minimum: 0,
          maximum: 1000,
        },
        showValue: true,
      },
      distortion: {
        title: "Distortion",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0,
          },
          minimum: -1000,
          maximum: 1000,
        },
        showValue: true,
      },
      factor: {
        title: "Factor",
        direction: "output",
        type: "float",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      color: {
        title: "Color",
        direction: "output",
        type: "color",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "parameter",
        parameter: "dimensions",
        title: "Dimensions",
      },
      {
        kind: "parameter",
        parameter: "noiseType",
        title: "Type",
      },
      {
        kind: "parameter",
        parameter: "normalize",
        title: "Normalize",
        visibleWhen: {
          parameter: "noiseType",
          equals: "fbm",
        },
      },
      {
        kind: "socket",
        socket: "vector",
        visibleWhen: {
          parameter: "dimensions",
          in: ["2d", "3d", "4d"],
        },
      },
      {
        kind: "socket",
        socket: "w",
        visibleWhen: {
          parameter: "dimensions",
          in: ["1d", "4d"],
        },
      },
      {
        kind: "socket",
        socket: "scale",
      },
      {
        kind: "socket",
        socket: "detail",
      },
      {
        kind: "socket",
        socket: "roughness",
      },
      {
        kind: "socket",
        socket: "lacunarity",
      },
      {
        kind: "socket",
        socket: "offset",
        visibleWhen: {
          parameter: "noiseType",
          in: ["hybrid-multifractal", "ridged-multifractal", "hetero-terrain"],
        },
      },
      {
        kind: "socket",
        socket: "gain",
        visibleWhen: {
          parameter: "noiseType",
          in: ["hybrid-multifractal", "ridged-multifractal"],
        },
      },
      {
        kind: "socket",
        socket: "distortion",
      },
      {
        kind: "socket",
        socket: "factor",
      },
      {
        kind: "socket",
        socket: "color",
      },
    ],
    muteBypass: [],
    migrations: [
      {
        fromVersion: 1,
        toVersion: 2,
        steps: [
          {
            kind: "materialize-missing",
            target: "parameter",
            key: "dimensions",
          },
          {
            kind: "materialize-missing",
            target: "parameter",
            key: "noiseType",
          },
          {
            kind: "materialize-missing",
            target: "parameter",
            key: "normalize",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "vector",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "w",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "scale",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "detail",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "roughness",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "lacunarity",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "offset",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "gain",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "distortion",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "factor",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "color",
          },
        ],
      },
    ],
  },
] as const satisfies readonly [string, FxNodeDefinition];
