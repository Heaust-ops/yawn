import type { FxNodeDefinition } from "@lib/composition/index.js";
export const imageNode = [
  "fxnode.compositor.image",
  {
    version: 1,
    title: "Image",
    behavior: "standard",
    style: "compositorInput",
    parameters: {
      image: {
        type: "string",
        default: {
          kind: "string",
          value: "",
        },
      },
      source: {
        type: "string",
        default: {
          kind: "string",
          value: "File",
        },
        enum: ["Generated", "File", "Movie", "Sequence", "Multilayer"],
      },
      frames: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
        minimum: 1,
        maximum: 1048574,
        integer: true,
      },
      startFrame: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
        minimum: -1048574,
        maximum: 1048574,
        integer: true,
      },
      offset: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
        minimum: -1048574,
        maximum: 1048574,
        integer: true,
      },
      cyclic: {
        type: "boolean",
        default: {
          kind: "boolean",
          value: false,
        },
      },
      autoRefresh: {
        type: "boolean",
        default: {
          kind: "boolean",
          value: false,
        },
      },
    },
    sockets: {
      image: {
        title: "Image",
        direction: "output",
        type: "color",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      alpha: {
        title: "Alpha",
        direction: "output",
        type: "float",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
      z: {
        title: "Z",
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
        kind: "resource",
        resource: "legacyImage",
        parameter: "image",
        title: "Image",
        openTitle: "Open",
      },
      {
        kind: "parameter",
        parameter: "source",
      },
      {
        kind: "parameter",
        parameter: "frames",
        title: "Frames",
        visibleWhen: {
          parameter: "source",
          in: ["Movie", "Sequence"],
        },
      },
      {
        kind: "parameter",
        parameter: "startFrame",
        title: "Start Frame",
        visibleWhen: {
          parameter: "source",
          in: ["Movie", "Sequence"],
        },
      },
      {
        kind: "parameter",
        parameter: "offset",
        visibleWhen: {
          parameter: "source",
          in: ["Movie", "Sequence"],
        },
      },
      {
        kind: "parameter",
        parameter: "cyclic",
        visibleWhen: {
          parameter: "source",
          in: ["Movie", "Sequence"],
        },
      },
      {
        kind: "parameter",
        parameter: "autoRefresh",
        title: "Auto Refresh",
        visibleWhen: {
          parameter: "source",
          in: ["Movie", "Sequence"],
        },
      },
      {
        kind: "socket",
        socket: "image",
      },
      {
        kind: "socket",
        socket: "alpha",
      },
      {
        kind: "socket",
        socket: "z",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
