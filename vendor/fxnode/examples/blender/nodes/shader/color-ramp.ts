import type { FxNodeDefinition } from "@lib/composition/index.js";
export const colorRampNode = [
  "fxnode.shader.color-ramp",
  {
    version: 2,
    title: "Color Ramp",
    behavior: "standard",
    style: "shader",
    parameters: {
      ramp: {
        type: "json",
        codec: "color-ramp/v1",
        default: {
          kind: "json",
          value: {
            colorMode: "rgb",
            interpolation: "linear",
            hueInterpolation: "near",
            stops: [
              {
                id: "stop-0",
                position: 0,
                color: [0, 0, 0, 1],
              },
              {
                id: "stop-1",
                position: 1,
                color: [1, 1, 1, 1],
              },
            ],
          },
        },
      },
    },
    sockets: {
      factor: {
        title: "Factor",
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
      color: {
        title: "Color",
        direction: "output",
        type: "color",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      alpha: {
        title: "Alpha",
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
        kind: "socket",
        socket: "color",
      },
      {
        kind: "socket",
        socket: "alpha",
      },
      {
        kind: "widget",
        widget: "color-ramp",
        parameter: "ramp",
        title: "",
      },
      {
        kind: "socket",
        socket: "factor",
      },
    ],
    muteBypass: [],
    migrations: [
      {
        fromVersion: 1,
        toVersion: 2,
        steps: [
          {
            kind: "materialize-missing",
            target: "parameter",
            key: "ramp",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "factor",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "color",
          },
          {
            kind: "materialize-missing",
            target: "socket",
            key: "alpha",
          },
          {
            kind: "migrate-parameter",
            parameter: "ramp",
            codec: "color-ramp/legacy-stops",
          },
        ],
      },
    ],
  },
] as const satisfies readonly [string, FxNodeDefinition];
