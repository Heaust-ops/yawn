import type { FxNodeDefinition } from "@lib/composition/index.js";
export const mathNode = [
  "fxnode.shader.math",
  {
    version: 1,
    title: "Math",
    behavior: "standard",
    style: "converter",
    parameters: {
      operation: {
        type: "string",
        default: {
          kind: "string",
          value: "add",
        },
        enum: ["add", "subtract", "multiply", "divide", "power", "minimum", "maximum"],
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
      a: {
        title: "A",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0,
          },
        },
        showValue: true,
      },
      b: {
        title: "B",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 0,
          },
        },
        showValue: true,
      },
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
        parameter: "operation",
      },
      {
        kind: "parameter",
        parameter: "clamp",
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
        socket: "value",
      },
    ],
    muteBypass: [["a", "value"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
