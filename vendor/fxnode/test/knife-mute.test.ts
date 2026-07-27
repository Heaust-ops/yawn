import assert from "node:assert/strict";
import test from "node:test";
import { appendKnifePoint, crossedLinks, MAX_KNIFE_POINTS, segmentsIntersect } from "@lib/worker/knife-path.js";
import { effectivelyMutedLinks } from "@lib/layout/link-mute.js";
import { layoutGraph as genericLayoutGraph } from "@lib/layout/layout-graph.js";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";
const layoutGraph = (
  document: Parameters<typeof genericLayoutGraph>[1],
  transform: Parameters<typeof genericLayoutGraph>[2],
) => genericLayoutGraph(APPLICATION_COMPILED, document, transform);
import { worldToView } from "@lib/layout/geometry.js";
const { emptyDocument, materializeNode } = APPLICATION_HEADLESS;

test("knife intersection handles directions, dedupes links, candidates, mute and point cap", () => {
  assert.equal(segmentsIntersect({ x: 0, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 }, { x: 10, y: 0 }), true);
  assert.equal(segmentsIntersect({ x: 10, y: 10 }, { x: 0, y: 0 }, { x: 0, y: 10 }, { x: 10, y: 0 }), true);
  assert.equal(segmentsIntersect({ x: 0, y: 0 }, { x: 2, y: 0 }, { x: 3, y: 0 }, { x: 5, y: 0 }), false);
  let points: readonly { x: number; y: number }[] = [];
  for (let i = 0; i < 300; i++) points = appendKnifePoint(points, { x: i * 3, y: 0 });
  assert.equal(points.length, MAX_KNIFE_POINTS);
  const source = materializeNode("source", "fxnode.shader.value", { x: -200, y: 50 }),
    target = materializeNode("target", "fxnode.shader.math", { x: 100, y: 50 });
  const base = emptyDocument();
  const document = {
    ...base,
    nodes: { source, target },
    links: {
      wire: {
        id: "wire",
        fromNodeId: "source",
        fromSocketId: source.sockets[0]!.id,
        toNodeId: "target",
        toSocketId: target.sockets[0]!.id,
        muted: false,
      },
    },
  } as never;
  const layout = layoutGraph(document, { center: { x: 0, y: 0 }, zoom: 1, viewport: { x: 1200, y: 640 }, dpr: 1 });
  const link = [...layout.links.values()].find((item) => item.visible && !item.muted)!;
  const middle = link.points[Math.floor(link.points.length / 2)]!;
  const view = worldToView(middle, layout.transform);
  const found = crossedLinks(layout, [
    { x: view.x, y: view.y - 100 },
    { x: view.x, y: view.y + 100 },
  ]);
  assert.equal(found.has(link.id), true);
  assert.equal(found.size, 1);
});

test("reroute effective mute propagates down chains and branches without changing authored flags", () => {
  const doc = {
    nodes: {
      a: { id: "a", typeId: "x", known: false, sockets: [] },
      r: { id: "r", typeId: "fxnode.common.reroute", known: true, sockets: [] },
      b: { id: "b", typeId: "x", known: false, sockets: [] },
    },
    links: {
      one: { id: "one", fromNodeId: "a", toNodeId: "r", muted: true },
      two: { id: "two", fromNodeId: "r", toNodeId: "b", muted: false },
      three: { id: "three", fromNodeId: "r", toNodeId: "a", muted: false },
    },
  } as never;
  assert.deepEqual([...effectivelyMutedLinks(APPLICATION_COMPILED, doc)].sort(), ["one", "three", "two"]);
  assert.equal((doc as any).links.two.muted, false);
});
