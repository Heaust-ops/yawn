import type { FxNodeDefinition } from "@lib/composition/index.js";
export const frameNode = [
  "fxnode.common.frame",
  {
    version: 1,
    title: "Frame",
    behavior: "frame",
    style: "common",
    parameters: {},
    sockets: {},
    ui: [],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
