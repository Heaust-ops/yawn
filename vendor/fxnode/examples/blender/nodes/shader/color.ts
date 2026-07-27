import type { FxNodeDefinition } from "@lib/composition/index.js";
export const colorNode = [
  "fxnode.shader.color",
  {
    version: 1,
    title: "Color",
    behavior: "standard",
    style: "shader",
    parameters: {
      color: {
        type: "color",
        default: {
          kind: "color",
          value: [0.8, 0.8, 0.8, 1],
        },
        minimum: 0,
        maximum: 1,
      },
    },
    sockets: {
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
        parameter: "color",
      },
      {
        kind: "socket",
        socket: "color",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
