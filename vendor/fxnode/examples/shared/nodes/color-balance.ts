import type { FxNodeDefinition } from "@lib/index.js";
/** Shared canonical presentation/schema example; it does not evaluate pixels. */
export const colorBalanceNode = [
  "fxnode.compositor.color-balance",
  {
    version: 1,
    title: "Color Balance",
    behavior: "standard",
    style: "compositorColor",
    parameters: {
      mode: {
        type: "string",
        default: {
          kind: "string",
          value: "Lift/Gamma/Gain",
        },
        enum: ["Lift/Gamma/Gain", "Offset/Power/Slope", "White Point"],
      },
      lift: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
      },
      liftColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      gamma: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
      },
      gammaColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      gain: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
      },
      gainColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      offset: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
      },
      offsetColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      power: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
      },
      powerColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      slope: {
        type: "number",
        default: {
          kind: "number",
          value: 1,
        },
      },
      slopeColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      inputTemperature: {
        type: "number",
        default: {
          kind: "number",
          value: 6500,
        },
      },
      inputTint: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
      },
      inputColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
      outputTemperature: {
        type: "number",
        default: {
          kind: "number",
          value: 6500,
        },
      },
      outputTint: {
        type: "number",
        default: {
          kind: "number",
          value: 0,
        },
      },
      outputColor: {
        type: "color",
        default: {
          kind: "color",
          value: [1, 1, 1, 1],
        },
      },
    },
    sockets: {
      image: {
        title: "Image",
        direction: "input",
        type: "color",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "color",
          default: {
            kind: "color",
            value: [0.8, 0.8, 0.8, 1],
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      factor: {
        title: "Factor",
        direction: "input",
        type: "float",
        maxIncomingLinks: 1,
        visible: true,
        value: {
          type: "number",
          default: {
            kind: "number",
            value: 1,
          },
          minimum: 0,
          maximum: 1,
        },
        showValue: true,
      },
      result: {
        title: "Image",
        direction: "output",
        type: "color",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [
      {
        kind: "parameter",
        parameter: "mode",
      },
      {
        kind: "widget",
        widget: "grading-wheels",
        bindings: [
          {
            title: "Lift",
            scalar: "lift",
            color: "liftColor",
          },
          {
            title: "Gamma",
            scalar: "gamma",
            color: "gammaColor",
          },
          {
            title: "Gain",
            scalar: "gain",
            color: "gainColor",
          },
        ],
        visibleWhen: {
          parameter: "mode",
          equals: "Lift/Gamma/Gain",
        },
      },
      {
        kind: "widget",
        widget: "grading-wheels",
        bindings: [
          {
            title: "Offset",
            scalar: "offset",
            color: "offsetColor",
          },
          {
            title: "Power",
            scalar: "power",
            color: "powerColor",
          },
          {
            title: "Slope",
            scalar: "slope",
            color: "slopeColor",
          },
        ],
        visibleWhen: {
          parameter: "mode",
          equals: "Offset/Power/Slope",
        },
      },
      {
        kind: "text",
        variant: "section",
        title: "Input",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "inputTemperature",
        title: "Temperature",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "inputTint",
        title: "Tint",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "inputColor",
        title: "Color",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "text",
        variant: "placeholder",
        title: "Eyedropper (host bridge)",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "text",
        variant: "section",
        title: "Output",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "outputTemperature",
        title: "Temperature",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "outputTint",
        title: "Tint",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "parameter",
        parameter: "outputColor",
        title: "Color",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "text",
        variant: "placeholder",
        title: "Eyedropper (host bridge)",
        visibleWhen: {
          parameter: "mode",
          equals: "White Point",
        },
      },
      {
        kind: "socket",
        socket: "image",
      },
      {
        kind: "socket",
        socket: "factor",
      },
      {
        kind: "socket",
        socket: "result",
      },
    ],
    muteBypass: [["image", "result"]],
    migrations: [],
  },
] as const satisfies readonly [string, FxNodeDefinition];
