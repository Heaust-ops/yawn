import type { FxNodeDefinition } from "@lib/composition/index.js";
export const transformGeometryNode = [
  "fxnode.geometry.transform-geometry",
  {
    version: 1,
    title: "Transform Geometry",
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
      translation: {
        title: "Translation",
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
      rotation: {
        title: "Rotation",
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
      scale: {
        title: "Scale",
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
        socket: "translation",
      },
      {
        kind: "socket",
        socket: "rotation",
      },
      {
        kind: "socket",
        socket: "scale",
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
