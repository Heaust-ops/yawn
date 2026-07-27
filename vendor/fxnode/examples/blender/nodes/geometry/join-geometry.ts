import type { FxNodeDefinition } from "@lib/composition/index.js";
export const joinGeometryNode = [
  "fxnode.geometry.join-geometry",
  {
    version: 1,
    title: "Join Geometry",
    behavior: "standard",
    style: "geometry",
    parameters: {},
    sockets: {
      geometry: {
        title: "Geometry",
        direction: "input",
        type: "geometry",
        maxIncomingLinks: 9007199254740991,
        visible: true,
        value: null,
        showValue: false,
      },
      result: {
        title: "Geometry",
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
        socket: "geometry",
      },
      {
        kind: "socket",
        socket: "result",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
