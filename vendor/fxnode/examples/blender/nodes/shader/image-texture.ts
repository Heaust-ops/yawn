import type { FxNodeDefinition } from "@lib/composition/index.js";
export const imageTextureNode = [
  "fxnode.shader.image-texture",
  {
    version: 1,
    title: "Image Texture",
    behavior: "standard",
    style: "input",
    parameters: {
      image: {
        type: "string",
        default: {
          kind: "string",
          value: "",
        },
      },
      interpolation: {
        type: "string",
        default: {
          kind: "string",
          value: "Linear",
        },
        enum: ["Linear", "Closest", "Cubic", "Smart"],
      },
      projection: {
        type: "string",
        default: {
          kind: "string",
          value: "Flat",
        },
        enum: ["Flat", "Box", "Sphere", "Tube"],
      },
      blend: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
        minimum: 0,
        maximum: 1,
      },
      extension: {
        type: "string",
        default: {
          kind: "string",
          value: "Repeat",
        },
        enum: ["Repeat", "Extend", "Clip", "Mirror"],
      },
      colorSpace: {
        type: "string",
        default: {
          kind: "string",
          value: "sRGB",
        },
        enum: ["sRGB", "Linear", "Non-Color"],
      },
      alphaMode: {
        type: "string",
        default: {
          kind: "string",
          value: "Straight",
        },
        enum: ["Straight", "Premultiplied", "Channel Packed", "None"],
      },
    },
    sockets: {
      vector: {
        title: "Vector",
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
        showValue: false,
      },
      color: {
        title: "Color",
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
        parameter: "interpolation",
      },
      {
        kind: "parameter",
        parameter: "projection",
      },
      {
        kind: "parameter",
        parameter: "blend",
        title: "Blend",
        visibleWhen: {
          parameter: "projection",
          equals: "Box",
        },
      },
      {
        kind: "parameter",
        parameter: "extension",
      },
      {
        kind: "parameter",
        parameter: "colorSpace",
        title: "Color Space",
      },
      {
        kind: "parameter",
        parameter: "alphaMode",
        title: "Alpha Mode",
      },
      {
        kind: "socket",
        socket: "vector",
      },
      {
        kind: "socket",
        socket: "color",
      },
      {
        kind: "socket",
        socket: "alpha",
      },
    ],
    muteBypass: [],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
