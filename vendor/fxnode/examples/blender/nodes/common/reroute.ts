import type { FxNodeDefinition } from "@lib/composition/index.js";
export const rerouteNode = [
  "fxnode.common.reroute",
  {
    version: 1,
    title: "Reroute",
    behavior: "reroute",
    style: "common",
    parameters: {},
    sockets: {
      input: {
        title: "Input",
        direction: "input",
        type: "any",
        maxIncomingLinks: 1,
        visible: true,
        value: null,
        showValue: false,
      },
      output: {
        title: "Output",
        direction: "output",
        type: "any",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "socket",
        socket: "input",
      },
      {
        kind: "socket",
        socket: "output",
      },
    ],
    muteBypass: [["input", "output"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
