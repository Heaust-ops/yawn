import { readFileSync, writeFileSync } from "node:fs";
import { APPLICATION_HEADLESS } from "../test/application.js";
import { APPLICATION_VERSION } from "../examples/blender/nodes/application.js";
import {
  graphId,
  linkId,
  nodeId,
  socketId,
  type GraphDocument,
  type GraphLink,
  type GraphNode,
} from "@lib/core/types.js";
import { nullRecord } from "@lib/core/json.js";

const { materializeNode, save } = APPLICATION_HEADLESS;
const node = (
  id: string,
  typeId: Parameters<typeof materializeNode>[1],
  position: { readonly x: number; readonly y: number },
  size: { readonly x: number; readonly y: number },
  options: { readonly label?: string; readonly collapsed?: boolean; readonly parentId?: string } = {},
): GraphNode => ({
  ...materializeNode(id, typeId, position),
  size,
  ...(options.label === undefined ? {} : { label: options.label }),
  ...(options.collapsed === undefined ? {} : { collapsed: options.collapsed }),
  ...(options.parentId === undefined ? {} : { parentId: nodeId(options.parentId) }),
});

const nodes = [
  node("frame", "fxnode.common.frame", { x: -550, y: 250 }, { x: 190, y: 270 }, { label: "Surface Controls" }),
  node("value-expanded", "fxnode.shader.value", { x: 30, y: -55 }, { x: 140, y: 100 }, { parentId: "frame" }),
  node(
    "value-collapsed",
    "fxnode.shader.value",
    { x: 30, y: -175 },
    { x: 140, y: 100 },
    { collapsed: true, parentId: "frame" },
  ),
  node("math", "fxnode.shader.math", { x: -300, y: 170 }, { x: 160, y: 110 }),
  node("noise", "fxnode.shader.noise-texture", { x: -100, y: 190 }, { x: 165, y: 130 }),
  node("reroute", "fxnode.common.reroute", { x: 105, y: 55 }, { x: 140, y: 100 }),
  node("principled", "fxnode.shader.principled-bsdf", { x: 165, y: 175 }, { x: 185, y: 125 }),
  node("output", "fxnode.shader.material-output", { x: 405, y: 115 }, { x: 155, y: 100 }),
];
const byId = nullRecord(nodes.map((item) => [item.id, item]));
const link = (id: string, fromNodeId: string, fromKey: string, toNodeId: string, toKey: string): GraphLink => ({
  id: linkId(id),
  fromNodeId: byId[fromNodeId]!.id,
  fromSocketId: socketId(`${fromNodeId}:${fromKey}`),
  toNodeId: byId[toNodeId]!.id,
  toSocketId: socketId(`${toNodeId}:${toKey}`),
  muted: false,
  extensions: {},
});
const links = [
  link("link-1", "value-expanded", "value", "math", "a"),
  link("link-2", "value-collapsed", "value", "math", "b"),
  link("link-3", "math", "value", "noise", "scale"),
  link("link-4", "noise", "factor", "reroute", "input"),
  link("link-5", "reroute", "output", "principled", "roughness"),
  link("link-6", "principled", "bsdf", "output", "surface"),
];
const document: GraphDocument = {
  schemaVersion: 2,
  graphId: graphId("phase-4-example"),
  catalogVersion: APPLICATION_VERSION,
  nodes: byId,
  links: nullRecord(links.map((item) => [item.id, item])),
  metadata: nullRecord([["description", "Deterministic fxnode browser baseline"]]),
};
const path = new URL("../examples/blender/initialLayout.json", import.meta.url);
const saved = save(document);
const { schemaVersion: _schemaVersion, ...layout } = saved;
const canonical = `${JSON.stringify({ ...layout, nodes: layout.nodes.map((node) => ({ ...node, known: true })) }, null, 2)}\n`;
if (process.argv.includes("--write")) writeFileSync(path, canonical);
else if (readFileSync(path, "utf8") !== canonical)
  throw new Error("Blender fixture is not canonical; run npm run generate:blender-fixture");
else console.log("blender fixture verified");
