import type { FxNodeDefinition, FxNodeSocketTypeDefinition, FxNodeStyleDefinition } from "@lib/index.js";
export const liveSocket = [
  "number",
  { title: "Number", color: "#74c0fc", acceptsFrom: ["number"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const liveStyles = { live: { header: "#0b7285" } } as const satisfies Readonly<
  Record<string, FxNodeStyleDefinition>
>;
const base = {
  title: "Live Parameter",
  behavior: "standard",
  style: "live",
  sockets: {
    result: {
      title: "Result",
      direction: "output",
      type: "number",
      maxIncomingLinks: 0,
      visible: true,
      value: null,
      showValue: false,
    },
  },
  muteBypass: [],
} as const;
export const liveNodeV1 = [
  "example.live.parameter",
  {
    ...base,
    version: 1,
    parameters: { value: { type: "number", default: { kind: "number", value: 1 } } },
    ui: [
      { kind: "parameter", parameter: "value" },
      { kind: "socket", socket: "result" },
    ],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
export const liveNodeV2 = [
  "example.live.parameter",
  {
    ...base,
    version: 2,
    parameters: {
      value: { type: "number", default: { kind: "number", value: 1 } },
      detail: { type: "number", default: { kind: "number", value: 0.5 }, minimum: 0, maximum: 1, step: 0.1 },
    },
    ui: [
      { kind: "parameter", parameter: "value" },
      { kind: "parameter", parameter: "detail" },
      { kind: "socket", socket: "result" },
    ],
    migrations: [
      { fromVersion: 1, toVersion: 2, steps: [{ kind: "materialize-missing", target: "parameter", key: "detail" }] },
    ],
  },
] as const satisfies readonly [string, FxNodeDefinition];
