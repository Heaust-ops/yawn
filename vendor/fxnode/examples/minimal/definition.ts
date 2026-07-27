import type { FxNodeDefinition, FxNodeSocketTypeDefinition, FxNodeStyleDefinition } from "@lib/index.js";
export const numberSocket = [
  "number",
  { title: "Number", color: "#a8a8a8", acceptsFrom: ["number"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const minimalStyles = { value: { header: "#4c6ef5" } } as const satisfies Readonly<
  Record<string, FxNodeStyleDefinition>
>;
export const valueNode = [
  "example.minimal.value",
  {
    version: 1,
    title: "Number Value",
    behavior: "standard",
    style: "value",
    parameters: { value: { type: "number", default: { kind: "number", value: 42 }, step: 1 } },
    sockets: {
      value: {
        title: "Value",
        direction: "output",
        type: "number",
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
