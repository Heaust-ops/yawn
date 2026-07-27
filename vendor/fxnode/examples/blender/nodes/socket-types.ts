import type { FxNodeSocketTypeDefinition } from "@lib/composition/index.js";
export const anySocket = ["any", { title: "any", color: "#999999", acceptsFrom: ["any"] }] as const satisfies readonly [
  string,
  FxNodeSocketTypeDefinition,
];
export const floatSocket = [
  "float",
  { title: "float", color: "#a8a8a8", acceptsFrom: ["float", "any"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const vectorSocket = [
  "vector",
  { title: "vector", color: "#6476dc", acceptsFrom: ["vector", "any"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const colorSocket = [
  "color",
  { title: "color", color: "#d7ca63", acceptsFrom: ["color", "any"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const shaderSocket = [
  "shader",
  { title: "shader", color: "#62b34f", acceptsFrom: ["shader", "any"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
export const geometrySocket = [
  "geometry",
  { title: "geometry", color: "#00bfa5", acceptsFrom: ["geometry", "any"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
