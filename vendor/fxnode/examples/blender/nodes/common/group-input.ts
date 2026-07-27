import type { FxNodeDefinition } from "@lib/composition/index.js";
export const groupInputNode = [
  "fxnode.common.group-input",
  {
    version: 1,
    title: "Group Input",
    behavior: "standard",
    style: "input",
    parameters: {
      interfaceName: {
        type: "string",
        default: {
          kind: "string",
          value: "Socket",
        },
      },
    },
    sockets: {
      output: {
        title: "Interface",
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
        kind: "parameter",
        parameter: "interfaceName",
      },
      {
        kind: "socket",
        socket: "output",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
