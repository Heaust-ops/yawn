import type { GraphNode, ParameterValue, Vec2 } from "../core/types.js";
import type { FxNodeDefinition, FxNodeUiRow, FxNodeValueSchema, FxNodeVisibility } from "../composition/types.js";
import { GEOMETRY as G } from "./constants.js";

const title = (value: string) => value.replace(/-/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
const textWidth = (value: string) => value.length * 6.5;
export const nodeRowUnits = (item: FxNodeUiRow): number =>
  item.kind === "text" && item.variant === "header"
    ? 2
    : item.kind === "widget"
      ? item.widget === "grading-wheels"
        ? 7
        : 8
      : item.kind === "resource"
        ? 4
        : 1;
const controlWidth = (schema: FxNodeValueSchema | undefined, ramp = false) =>
  !schema
    ? 80
    : schema.type === "vector"
      ? 180
      : schema.type === "color"
        ? 80
        : ramp
          ? 300
          : schema.type === "string"
            ? Math.max(80, ...(schema.enum ?? []).map((value) => textWidth(value) + 28))
            : 80;
export const visibleWhen = (expression: FxNodeVisibility | undefined, values: GraphNode["parameters"]): boolean =>
  !expression
    ? true
    : "all" in expression
      ? expression.all.every((item) => visibleWhen(item, values))
      : "any" in expression
        ? expression.any.some((item) => visibleWhen(item, values))
        : "equals" in expression
          ? (values[expression.parameter] as ParameterValue | undefined)?.value === expression.equals
          : expression.in.includes((values[expression.parameter] as ParameterValue | undefined)?.value as never);
export const visibleNodeItems = (
  definition: FxNodeDefinition,
  node: Pick<GraphNode, "parameters" | "sockets">,
): readonly FxNodeUiRow[] =>
  definition.ui
    .filter((item) => item.kind !== "hidden" && visibleWhen(item.visibleWhen, node.parameters))
    .filter(
      (item) => item.kind !== "socket" || node.sockets.some((socket) => socket.key === item.socket && socket.visible),
    );
export function minimumNodeSize(
  definition: FxNodeDefinition,
  node: Pick<GraphNode, "label" | "parameters" | "sockets">,
): Vec2 {
  if (definition.behavior === "reroute") return { x: 10, y: 10 };
  if (definition.behavior === "frame") return { x: G.frameMinimum, y: G.frameMinimum };
  const items = visibleNodeItems(definition, node);
  let width = Math.max(G.minWidth, textWidth(node.label) + 42);
  for (const item of items) {
    const label =
      "title" in item
        ? item.title
        : item.kind === "parameter" || item.kind === "resource"
          ? title(item.parameter)
          : item.kind === "socket"
            ? title(item.socket)
            : "";
    if (label) width = Math.max(width, (textWidth(label) + 18) / 0.4);
    if (item.kind === "parameter" || item.kind === "resource") {
      const schema = definition.parameters[item.parameter];
      width = Math.max(
        width,
        controlWidth(
          schema,
          item.kind === "parameter" &&
            definition.ui.some(
              (r) => r.kind === "widget" && r.widget === "color-ramp" && r.parameter === item.parameter,
            ),
        ) / 0.53,
      );
    } else if (item.kind === "socket") {
      const socket = definition.sockets[item.socket];
      if (socket?.value && socket.showValue) width = Math.max(width, controlWidth(socket.value) / 0.53);
    } else if (item.kind === "widget") width = Math.max(width, item.widget === "grading-wheels" ? 400 : 320);
  }
  return {
    x: Math.min(G.maxWidth, Math.ceil(width)),
    y: G.header + items.reduce((sum, item) => sum + nodeRowUnits(item), 0) * G.row + G.gap,
  };
}
export function initialNodeSize(
  definition: FxNodeDefinition,
  node: Pick<GraphNode, "label" | "parameters" | "sockets">,
): Vec2 {
  if (definition.behavior === "frame") return { x: G.initialFrameWidth, y: G.initialNodeHeight };
  if (definition.behavior === "reroute") return { x: G.reroute * 2, y: G.reroute * 2 };
  const minimum = minimumNodeSize(definition, node);
  return { x: Math.max(G.initialNodeWidth, minimum.x), y: Math.max(G.initialNodeHeight, minimum.y) };
}
