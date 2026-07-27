/** Compile-only mirror of README composition snippets; check:readme guards their key syntax. */
import type { FxNode, FxNodeDefinition, FxNodeSocketTypeDefinition, FxNodeStyleDefinition } from "@lib/index.js";
import { createFxNodeHeadless } from "@lib/headless.js";
import { minimalStyles, numberSocket, valueNode } from "../examples/minimal/definition.js";
import { colorBalanceNode } from "../examples/shared/nodes/color-balance.js";
import { exampleTheme } from "../examples/shared/theme.js";

const floatSocket = [
  "float",
  { title: "Float", color: "#a8a8a8", acceptsFrom: ["float"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
const styles = { compositorColor: { header: "#8c5cc4" } } as const satisfies Readonly<
  Record<string, FxNodeStyleDefinition>
>;
export async function installColorBalance(api: FxNode) {
  await api.setTheme(exampleTheme);
  await api.setHeaderStyles(styles);
  await api.composeSocket(...floatSocket);
  await api.composeNode(...colorBalanceNode);
}
export const gradingWheelsRow = {
  kind: "widget",
  widget: "grading-wheels",
  bindings: [
    { title: "Lift", scalar: "lift", color: "liftColor" },
    { title: "Gamma", scalar: "gamma", color: "gammaColor" },
    { title: "Gain", scalar: "gain", color: "gainColor" },
  ],
  visibleWhen: { parameter: "mode", equals: "Lift/Gamma/Gain" },
} satisfies FxNodeDefinition["ui"][number];
export const readmeHeadless = createFxNodeHeadless({
  schemaVersion: 2,
  id: "fxnode.example.minimal",
  version: 1,
  compatibility: { wildcardInputTypes: [] },
  theme: exampleTheme,
  socketTypes: { [numberSocket[0]]: numberSocket[1] },
  nodeStyles: minimalStyles,
  resources: {},
  nodes: { [valueNode[0]]: valueNode[1] },
} as const);
