import type { FxNodeDefinition } from "@lib/composition/index.js";
export const meshCubeNode = [
  "fxnode.geometry.mesh-cube",
  {
    version: 1,
    title: "Mesh Cube",
    behavior: "standard",
    style: "geometry",
    parameters: {},
    sockets: {
      size: {
        title: "Size",
        direction: "input",
        type: "vector",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "vector",
          default: {
            kind: "vector",
            value: [1, 1, 1],
          },
          minimum: 0,
        },
        showValue: true,
      },
      "vertices-x": {
        title: "Vertices X",
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
          minimum: 2,
          maximum: 1000,
          integer: true,
        },
        showValue: true,
      },
      "vertices-y": {
        title: "Vertices Y",
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
          minimum: 2,
          maximum: 1000,
          integer: true,
        },
        showValue: true,
      },
      "vertices-z": {
        title: "Vertices Z",
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
          minimum: 2,
          maximum: 1000,
          integer: true,
        },
        showValue: true,
      },
      mesh: {
        title: "Mesh",
        direction: "output",
        type: "geometry",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "socket",
        socket: "size",
      },
      {
        kind: "socket",
        socket: "vertices-x",
      },
      {
        kind: "socket",
        socket: "vertices-y",
      },
      {
        kind: "socket",
        socket: "vertices-z",
      },
      {
        kind: "socket",
        socket: "mesh",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
