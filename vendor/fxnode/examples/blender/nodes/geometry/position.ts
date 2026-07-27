import type { FxNodeDefinition } from "@lib/composition/index.js";
export const positionNode = [
  "fxnode.geometry.position",
  {
    version: 1,
    title: "Position",
    behavior: "standard",
    style: "geometry",
    parameters: {},
    sockets: {
      position: {
        title: "Position",
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
        socket: "position",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
