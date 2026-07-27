import type { FxNodeDefinition } from "@lib/composition/index.js";
export const mixNode = [
  "fxnode.shader.mix",
  {
    version: 1,
    title: "Mix",
    behavior: "standard",
    style: "shader",
    parameters: {
      blend: {
        type: "string",
        default: {
          kind: "string",
          value: "mix",
        },
        enum: ["mix", "add", "multiply", "screen", "overlay"],
      },
      clamp: {
        type: "boolean",
        default: {
          kind: "boolean",
          value: false,
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
      a: {
        title: "A",
        direction: "input",
        type: "color",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "color",
          default: {
            kind: "color",
            value: [0.8, 0.8, 0.8, 1],
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      b: {
        title: "B",
        direction: "input",
        type: "color",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "color",
          default: {
            kind: "color",
            value: [0.8, 0.8, 0.8, 1],
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      result: {
        title: "Result",
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
        parameter: "blend",
      },
      {
        kind: "parameter",
        parameter: "clamp",
      },
      {
        kind: "socket",
        socket: "factor",
      },
      {
        kind: "socket",
        socket: "a",
      },
      {
        kind: "socket",
        socket: "b",
      },
      {
        kind: "socket",
        socket: "result",
      },
    ],
    muteBypass: [["a", "result"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
