import type { FxNodeDefinition } from "@lib/composition/index.js";
export const textureCoordinateNode = [
  "fxnode.shader.texture-coordinate",
  {
    version: 1,
    title: "Texture Coordinate",
    behavior: "standard",
    style: "shader",
    parameters: {},
    sockets: {
      generated: {
        title: "Generated",
        direction: "output",
        type: "vector",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      normal: {
        title: "Normal",
        direction: "output",
        type: "vector",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      uv: {
        title: "UV",
        direction: "output",
        type: "vector",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      object: {
        title: "Object",
        direction: "output",
        type: "vector",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "socket",
        socket: "generated",
      },
      {
        kind: "socket",
        socket: "normal",
      },
      {
        kind: "socket",
        socket: "uv",
      },
      {
        kind: "socket",
        socket: "object",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
