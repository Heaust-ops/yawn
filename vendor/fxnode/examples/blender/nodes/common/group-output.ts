import type { FxNodeDefinition } from "@lib/composition/index.js";
export const groupOutputNode = [
  "fxnode.common.group-output",
  {
    version: 1,
    title: "Group Output",
    behavior: "standard",
    style: "output",
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
      input: {
        title: "Interface",
        direction: "input",
        type: "any",
        maxIncomingLinks: 9007199254740991,
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
        socket: "input",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
