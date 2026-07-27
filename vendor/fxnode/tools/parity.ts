import { writeFileSync } from "node:fs";
import { APPLICATION_COMPILED } from "../test/application.js";
import { APPLICATION_HEADLESS } from "../test/application.js";
import type { NodeTypeId } from "@lib/composition/index.js";
import { graphId, linkId, socketId, type GraphDocument } from "@lib/core/types.js";
import { nullRecord } from "@lib/core/json.js";

const { materializeNode, save } = APPLICATION_HEADLESS;
const specs: readonly [string, NodeTypeId<typeof APPLICATION_COMPILED.source>, number, number][] = [
  ["image-texture", "fxnode.shader.image-texture", -520, 180],
  ["noise-3d", "fxnode.shader.noise-texture", -220, 180],
  ["noise-4d", "fxnode.shader.noise-texture", 60, 180],
  ["color-ramp", "fxnode.shader.color-ramp", 350, 180],
  ["compositor-image", "fxnode.compositor.image", -340, -260],
  ["master", "fxnode.compositor.color-balance", 20, -260],
];
const nodes = specs.map(([id, type, x, y]) => {
  const node = materializeNode(id, type, { x, y });
  if (id === "noise-4d")
    return { ...node, parameters: { ...node.parameters, dimensions: { kind: "string" as const, value: "4d" } } };
  if (id === "master") return { ...node, label: "Master Color Grading" };
  return node;
});
const link = {
  id: linkId("compositor-grade"),
  fromNodeId: nodes[4]!.id,
  fromSocketId: socketId("compositor-image:image"),
  toNodeId: nodes[5]!.id,
  toSocketId: socketId("master:image"),
  muted: false,
  extensions: {},
};
const document: GraphDocument = {
  schemaVersion: 2,
  graphId: graphId("parity"),
  catalogVersion: APPLICATION_COMPILED.source.version,
  nodes: nullRecord(nodes.map((n) => [n.id, n])),
  links: nullRecord([[link.id, link]]),
  metadata: nullRecord(),
};
const saved = save(document);
const { schemaVersion: _schemaVersion, ...layout } = saved;
writeFileSync(
  new URL("../examples/blender/parity/initialLayout.json", import.meta.url),
  JSON.stringify({ ...layout, nodes: layout.nodes.map((node) => ({ ...node, known: true })) }, null, 2) + "\n",
);
