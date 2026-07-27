import type { FxNodeDefinition } from "@lib/composition/index.js";
export const principledBsdfNode = [
  "fxnode.shader.principled-bsdf",
  {
    version: 1,
    title: "Principled BSDF",
    behavior: "standard",
    style: "shader",
    parameters: {},
    sockets: {
      "base-color": {
        title: "Base Color",
        direction: "input",
        type: "color",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "color",
          default: {
            kind: "color",
            value: [0.8, 0.8, 0.8, 1],
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      metallic: {
        title: "Metallic",
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
          minimum: 0,
          maximum: 1,
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
      ior: {
        title: "IOR",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 1.5,
          },
          minimum: 1,
          maximum: 1000,
        },
        showValue: true,
      },
      alpha: {
        title: "Alpha",
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
          maximum: 1,
        },
        showValue: true,
      },
      normal: {
        title: "Normal",
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
      bsdf: {
        title: "BSDF",
        direction: "output",
        type: "shader",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "socket",
        socket: "base-color",
      },
      {
        kind: "socket",
        socket: "metallic",
      },
      {
        kind: "socket",
        socket: "roughness",
      },
      {
        kind: "socket",
        socket: "ior",
      },
      {
        kind: "socket",
        socket: "alpha",
      },
      {
        kind: "socket",
        socket: "normal",
      },
      {
        kind: "socket",
        socket: "bsdf",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
