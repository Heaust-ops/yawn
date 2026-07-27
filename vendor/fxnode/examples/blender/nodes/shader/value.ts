import type { FxNodeDefinition } from "@lib/composition/index.js";
export const valueNode = [
  "fxnode.shader.value",
  {
    version: 1,
    title: "Value",
    behavior: "standard",
    style: "shader",
    parameters: {
      value: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
        step: 1,
      },
    },
    sockets: {
      value: {
        title: "Value",
        direction: "output",
        type: "float",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "parameter",
        parameter: "value",
      },
      {
        kind: "socket",
        socket: "value",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
