import type { FxNodeDefinition } from "@lib/composition/index.js";
export const materialOutputNode = [
  "fxnode.shader.material-output",
  {
    version: 1,
    title: "Material Output",
    behavior: "standard",
    style: "output",
    parameters: {},
    sockets: {
      surface: {
        title: "Surface",
        direction: "input",
        type: "shader",
        maxIncomingLinks: 1,
        visible: true,
        value: null,
        showValue: false,
      },
      volume: {
        title: "Volume",
        direction: "input",
        type: "shader",
        maxIncomingLinks: 1,
        visible: true,
        value: null,
        showValue: false,
      },
      displacement: {
        title: "Displacement",
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
        showValue: true,
      },
    },
    ui: [
      {
        kind: "socket",
        socket: "surface",
      },
      {
        kind: "socket",
        socket: "volume",
      },
      {
        kind: "socket",
        socket: "displacement",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
