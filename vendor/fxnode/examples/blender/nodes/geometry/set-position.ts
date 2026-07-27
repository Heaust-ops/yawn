import type { FxNodeDefinition } from "@lib/composition/index.js";
export const setPositionNode = [
  "fxnode.geometry.set-position",
  {
    version: 1,
    title: "Set Position",
    behavior: "standard",
    style: "geometry",
    parameters: {},
    sockets: {
      geometry: {
        title: "Geometry",
        direction: "input",
        type: "geometry",
        maxIncomingLinks: 1,
        visible: true,
        value: null,
        showValue: false,
      },
      position: {
        title: "Position",
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
      offset: {
        title: "Offset",
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
        socket: "position",
      },
      {
        kind: "socket",
        socket: "offset",
      },
      {
        kind: "socket",
        socket: "result",
      },
    ],
    muteBypass: [["geometry", "result"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
