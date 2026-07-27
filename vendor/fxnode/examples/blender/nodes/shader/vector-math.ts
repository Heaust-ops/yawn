import type { FxNodeDefinition } from "@lib/composition/index.js";
export const vectorMathNode = [
  "fxnode.shader.vector-math",
  {
    version: 1,
    title: "Vector Math",
    behavior: "standard",
    style: "converter",
    parameters: {
      operation: {
        type: "string",
        default: {
          kind: "string",
          value: "add",
        },
        enum: ["add", "subtract", "multiply", "dot-product", "length", "normalize"],
      },
    },
    sockets: {
      a: {
        title: "A",
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
      b: {
        title: "B",
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
      vector: {
        title: "Vector",
        direction: "output",
        type: "vector",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
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
        kind: "socket",
        socket: "a",
      },
      {
        kind: "socket",
        socket: "b",
        visibleWhen: {
          parameter: "operation",
          equals: "add",
        },
      },
      {
        kind: "socket",
        socket: "vector",
      },
      {
        kind: "socket",
        socket: "value",
      },
    ],
    muteBypass: [["a", "vector"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
