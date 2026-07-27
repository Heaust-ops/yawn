/** Shared constants for the example application composition. */
import type { FxNodeCompositionData } from "@lib/composition/index.js";
export const applicationCompatibility = {
  wildcardInputTypes: ["any"],
} as const satisfies FxNodeCompositionData["compatibility"];
export const APPLICATION_ID = "fxnode.example.application";
export const APPLICATION_VERSION = 4;
export const APPLICATION_HEADER_STYLES = {
  input: {
    header: "#8b3f72",
  },
  converter: {
    header: "#4f5964",
  },
  texture: {
    header: "#a36b34",
  },
  shader: {
    header: "#3b7551",
  },
  output: {
    header: "#963d3d",
  },
  geometry: {
    header: "#2c7a75",
  },
  common: {
    header: "#555b64",
  },
  compositorInput: {
    header: "#8b3f72",
  },
  compositorColor: {
    header: "#4f5964",
  },
} as const satisfies FxNodeCompositionData["nodeStyles"];
export const APPLICATION_RESOURCES = {
  legacyImage: {
    kind: "image",
    title: "Image",
    openTitle: "Open",
    accept: ["image/*"],
    referencePrefix: "fxnode-local-image:",
    maxBytes: 33554432,
    maxWidth: 8192,
    maxHeight: 8192,
    maxPixels: 16777216,
  },
} as const satisfies FxNodeCompositionData["resources"];
