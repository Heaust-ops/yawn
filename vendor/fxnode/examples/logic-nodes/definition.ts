import type { FxNodeDefinition, FxNodeSocketTypeDefinition, FxNodeStyleDefinition } from "@lib/index.js";

export const booleanSocket = [
  "boolean",
  { title: "Boolean", color: "#d67cff", acceptsFrom: ["boolean"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];

export const logicStyles = {
  source: { header: "#547aa5" },
  logic: { header: "#7c4d9e" },
} as const satisfies Readonly<Record<string, FxNodeStyleDefinition>>;

export const booleanValueNode = [
  "example.logic.boolean",
  {
    version: 1,
    title: "Boolean",
    behavior: "standard",
    style: "source",
    parameters: {
      value: { type: "boolean", default: { kind: "boolean", value: true } },
    },
    sockets: {
      value: {
        title: "Value",
        direction: "output",
        type: "boolean",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      { kind: "parameter", parameter: "value" },
      { kind: "socket", socket: "value" },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];

function gateNode(title: string, inputCapacity: number): FxNodeDefinition {
  return {
    version: 1,
    title,
    behavior: "standard",
    style: "logic",
    parameters: {},
    sockets: {
      inputs: {
        title: inputCapacity === 1 ? "Input" : `Inputs (up to ${inputCapacity})`,
        direction: "input",
        type: "boolean",
        maxIncomingLinks: inputCapacity,
        visible: true,
        value: null,
        showValue: false,
      },
      result: {
        title: "Result",
        direction: "output",
        type: "boolean",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      { kind: "socket", socket: "inputs" },
      { kind: "socket", socket: "result" },
    ],
    muteBypass: [["inputs", "result"]],
    migrations: [],
  };
}

export const logicNodes = [
  ["example.logic.and", gateNode("AND", 5)],
  ["example.logic.or", gateNode("OR", 5)],
  ["example.logic.not", gateNode("NOT", 1)],
  ["example.logic.xor", gateNode("XOR", 5)],
  ["example.logic.xnor", gateNode("XNOR", 5)],
  ["example.logic.nand", gateNode("NAND", 5)],
  ["example.logic.nor", gateNode("NOR", 5)],
] as const satisfies readonly (readonly [string, FxNodeDefinition])[];
