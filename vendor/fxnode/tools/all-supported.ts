import { readFileSync, writeFileSync } from "node:fs";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "../test/application.js";
import { graphId, linkId, socketId, type GraphDocument, type GraphLink } from "@lib/core/types.js";
import { nullRecord } from "@lib/core/json.js";

const { decodeGraphDocument, materializeNode, save } = APPLICATION_HEADLESS;
const definitions = [...APPLICATION_COMPILED.nodes.values()];
const positions = { common: 0, shader: -280, geometry: -560, compositor: -840 };
// This visual fixture intentionally pins authored document sizes. Fresh nodes use calculated dimensions.
const authoredSizes: Readonly<Record<string, { readonly x: number; readonly y: number }>> = {
  "fxnode.common.frame": { x: 300, y: 100 },
  "fxnode.common.reroute": { x: 140, y: 100 },
  "fxnode.common.group-input": { x: 180, y: 100 },
  "fxnode.common.group-output": { x: 180, y: 100 },
  "fxnode.shader.value": { x: 140, y: 100 },
  "fxnode.shader.color": { x: 140, y: 100 },
  "fxnode.shader.math": { x: 180, y: 100 },
  "fxnode.shader.vector-math": { x: 190, y: 100 },
  "fxnode.shader.mix": { x: 190, y: 100 },
  "fxnode.shader.color-ramp": { x: 320, y: 100 },
  "fxnode.shader.texture-coordinate": { x: 190, y: 100 },
  "fxnode.shader.noise-texture": { x: 200, y: 100 },
  "fxnode.shader.image-texture": { x: 280, y: 100 },
  "fxnode.shader.principled-bsdf": { x: 220, y: 100 },
  "fxnode.shader.material-output": { x: 190, y: 100 },
  "fxnode.geometry.position": { x: 140, y: 100 },
  "fxnode.geometry.mesh-cube": { x: 190, y: 100 },
  "fxnode.geometry.set-position": { x: 190, y: 100 },
  "fxnode.geometry.transform-geometry": { x: 210, y: 100 },
  "fxnode.geometry.join-geometry": { x: 180, y: 100 },
  "fxnode.compositor.image": { x: 240, y: 100 },
  "fxnode.compositor.color-balance": { x: 400, y: 100 },
};
const nodes = definitions.map((definition) => {
  const family = definition.typeId.split(".")[1] as keyof typeof positions;
  const peers = definitions.filter((item) => item.typeId.split(".")[1] === family);
  const materialized = materializeNode(`all-${definition.typeId.replaceAll(".", "-")}`, definition.typeId, {
    x: 40 + peers.indexOf(definition) * 240,
    y: positions[family],
  });
  const node = { ...materialized, size: authoredSizes[definition.typeId] ?? materialized.size };
  if (definition.typeId === "fxnode.shader.color-ramp")
    return {
      ...node,
      parameters: {
        ramp: {
          kind: "json" as const,
          value: {
            colorMode: "hsv",
            interpolation: "ease",
            hueInterpolation: "far",
            stops: [
              { id: "black", position: 0, color: [0, 0, 0, 1] },
              { id: "accent", position: 0.38, color: [0.05, 0.35, 1, 1] },
              { id: "white", position: 1, color: [1, 1, 1, 1] },
            ],
          },
        },
      },
    } as typeof node;
  if (definition.typeId === "fxnode.shader.noise-texture")
    return {
      ...node,
      parameters: {
        ...node.parameters,
        dimensions: { kind: "string" as const, value: "4d" },
        noiseType: { kind: "string" as const, value: "hybrid-multifractal" },
      },
    } as typeof node;
  return node;
});
const byType = (type: string) => nodes.find((node) => node.typeId === type)!;
const cube = byType("fxnode.geometry.mesh-cube"),
  position = byType("fxnode.geometry.position"),
  set = byType("fxnode.geometry.set-position"),
  transform = byType("fxnode.geometry.transform-geometry"),
  join = byType("fxnode.geometry.join-geometry");
const make = (id: string, from: typeof cube, fromKey: string, to: typeof join, toKey: string): GraphLink => ({
  id: linkId(id),
  fromNodeId: from.id,
  fromSocketId: socketId(`${from.id}:${fromKey}`),
  toNodeId: to.id,
  toSocketId: socketId(`${to.id}:${toKey}`),
  muted: false,
  extensions: {},
});
const links = [
  make("all-link-position", position, "position", set, "position"),
  make("all-link-cube", cube, "mesh", set, "geometry"),
  make("all-link-set", set, "result", join, "geometry"),
  make("all-link-transform", transform, "result", join, "geometry"),
];
const document: GraphDocument = {
  schemaVersion: 2,
  graphId: graphId("all-supported"),
  catalogVersion: APPLICATION_COMPILED.source.version,
  nodes: nullRecord(nodes.map((node) => [node.id, node])),
  links: nullRecord(links.map((link) => [link.id, link])),
  metadata: nullRecord(),
};
const saved = save(document);
const { schemaVersion: _schemaVersion, ...persistedLayout } = saved;
export const initialLayout = {
  ...persistedLayout,
  nodes: persistedLayout.nodes.map((node) => ({ ...node, known: true })),
};
const path = new URL("../examples/blender/all-supported/initialLayout.json", import.meta.url),
  canonical = JSON.stringify(initialLayout, null, 2) + "\n";
if (process.argv.includes("--write")) writeFileSync(path, canonical);
else {
  const bytes = readFileSync(path, "utf8"),
    actual = JSON.parse(bytes),
    decoded = decodeGraphDocument({
      ...actual,
      schemaVersion: 2,
      nodes: actual.nodes.map(({ known: _, ...node }: any) => node),
    });
  if (!decoded.ok) throw new Error(`Initial layout decode failed: ${JSON.stringify(decoded.issues)}`);
  const ids = actual.nodes.map((node: { typeId: string }) => node.typeId),
    expected = definitions.length;
  if (
    ids.length !== expected ||
    new Set(ids).size !== expected ||
    definitions.some((item) => !ids.includes(item.typeId))
  )
    throw new Error("Fixture must contain each application type exactly once");
  if (JSON.stringify(actual) !== JSON.stringify(initialLayout) || bytes !== canonical)
    throw new Error("Initial layout is not canonical; run generate:all-supported");
  if (actual.links.filter((link: { toNodeId: string }) => link.toNodeId === join.id).length < 2)
    throw new Error("Join Geometry needs two incoming links");
  console.log("all-supported fixture verified");
}
