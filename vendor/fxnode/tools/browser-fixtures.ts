import { readFileSync, writeFileSync } from "node:fs";
import { APPLICATION_HEADLESS } from "../test/application.js";
import { APPLICATION_VERSION } from "../examples/blender/nodes/application.js";

const { materializeNode } = APPLICATION_HEADLESS;
const plain = (id: string, type: Parameters<typeof materializeNode>[1], x: number, y: number) => {
  return materializeNode(id, type, { x, y });
};
const byId = <T extends { readonly id: string }>(items: readonly T[]): T[] =>
  [...items].sort((left, right) => left.id.localeCompare(right.id));
const fixtures: Record<string, unknown> = {
  "control-test": {
    graphId: "control-test",
    catalogVersion: APPLICATION_VERSION,
    nodes: byId([
      plain("value", "fxnode.shader.value", -500, 200),
      plain("math", "fxnode.shader.math", -250, 200),
      plain("vector", "fxnode.shader.vector-math", 20, 200),
      plain("color", "fxnode.shader.color", 300, 200),
      plain("group", "fxnode.common.group-input", -100, -100),
    ]),
    links: [
      {
        id: "value-math",
        fromNodeId: "value",
        fromSocketId: "value:value",
        toNodeId: "math",
        toSocketId: "math:a",
        muted: false,
        extensions: {},
      },
    ],
    metadata: {},
  },
  "link-tools-test": (() => {
    const nodes = [
      plain("source-a", "fxnode.shader.value", -560, 240),
      plain("math-a", "fxnode.shader.math", 180, 240),
      plain("source-b", "fxnode.shader.value", -560, 120),
      plain("math-b", "fxnode.shader.math", 180, 120),
      plain("source-c", "fxnode.shader.value", -560, 0),
      plain("math-c", "fxnode.shader.math", 180, 0),
      plain("chain-source", "fxnode.shader.value", -560, -100),
      plain("reroute", "fxnode.common.reroute", 0, -100),
      plain("chain-math", "fxnode.shader.math", 260, -100),
      plain("transform", "fxnode.geometry.transform-geometry", -300, -230),
      plain("noise", "fxnode.shader.noise-texture", 180, -230),
    ];
    const links = [
      ["parallel-a", "source-a", "source-a:value", "math-a", "math-a:a"],
      ["parallel-b", "source-b", "source-b:value", "math-b", "math-b:a"],
      ["parallel-c", "source-c", "source-c:value", "math-c", "math-c:a"],
      ["chain-in", "chain-source", "chain-source:value", "reroute", "reroute:input"],
      ["chain-out", "reroute", "reroute:output", "chain-math", "chain-math:a"],
    ] as const;
    return {
      graphId: "link-tools-test",
      catalogVersion: APPLICATION_VERSION,
      nodes: byId(nodes),
      links: byId(
        links.map(([id, fromNodeId, fromSocketId, toNodeId, toSocketId]) => ({
          id,
          fromNodeId,
          fromSocketId,
          toNodeId,
          toSocketId,
          muted: false,
          extensions: {},
        })),
      ),
      metadata: {},
    };
  })(),
  "ramp-test": (() => {
    const frame = plain("frame", "fxnode.common.frame", -300, 250),
      ramp = plain("ramp", "fxnode.shader.color-ramp", 40, -40),
      value = {
        colorMode: "rgb",
        interpolation: "linear",
        hueInterpolation: "near",
        stops: [
          { id: "red", position: 0.1, color: [1, 0, 0, 1] },
          { id: "green", position: 0.35, color: [0, 1, 0, 0.8] },
          { id: "blue", position: 0.9, color: [0, 0, 1, 0.6] },
        ],
      };
    return {
      graphId: "ramp-test",
      catalogVersion: APPLICATION_VERSION,
      nodes: [
        { ...frame, size: { x: 380, y: 310 } },
        { ...ramp, parentId: "frame", parameters: { ...ramp.parameters, ramp: { kind: "json", value } } },
      ],
      links: [],
      metadata: {},
    };
  })(),
};
for (const [name, fixture] of Object.entries(fixtures)) {
  const url = new URL(`../examples/blender/${name}/initialLayout.json`, import.meta.url),
    canonical = `${JSON.stringify(fixture, null, 2)}\n`;
  if (process.argv.includes("--write")) writeFileSync(url, canonical);
  else if (readFileSync(url, "utf8") !== canonical)
    throw new Error(`${name} fixture is not canonical; run npm run generate:browser-fixtures`);
}
if (!process.argv.includes("--write")) console.log("browser fixtures verified");
