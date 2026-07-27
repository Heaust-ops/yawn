import assert from "node:assert/strict";
import test from "node:test";
import {
  planAtlasCompaction,
  planAtlasUpsert,
  removeAtlasItem,
  type AtlasLayout,
} from "@lib/worker/atlas-allocator.js";

const add = (layout: AtlasLayout | undefined, id: string, width: number, height: number) => {
  const plan = planAtlasUpsert(layout, { id, width, height });
  assert.equal(plan.ok, true);
  return plan.ok ? plan.layout : undefined!;
};
const overlaps = (a: { x: number; y: number; width: number; height: number }, b: typeof a) =>
  a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
function valid(layout: AtlasLayout): void {
  const regions = [...layout.regions.values()],
    free = [...layout.free];
  regions.forEach((region, index) => {
    assert(region.x >= 0 && region.y >= 0);
    assert(region.x + region.width <= layout.width);
    assert(region.y + region.height <= layout.height);
    regions.slice(index + 1).forEach((other) => assert.equal(overlaps(region, other), false));
    free.forEach((other) => assert.equal(overlaps(region, other), false));
  });
  free.forEach((region, index) => {
    assert(region.x >= 0 && region.y >= 0);
    assert(region.x + region.width <= layout.width);
    assert(region.y + region.height <= layout.height);
    free.slice(index + 1).forEach((other) => assert.equal(overlaps(region, other), false));
  });
  const covered = [...regions, ...free].reduce((sum, region) => sum + region.width * region.height, 0);
  assert.equal(covered, layout.width * layout.height);
}

test("atlas allocation is deterministic, bounded, and preserves shrink capacity", () => {
  const first = add(undefined, "a", 100, 80);
  assert.deepEqual(
    { width: first.width, height: first.height, region: first.regions.get("a") },
    {
      width: 256,
      height: 256,
      region: { x: 0, y: 0, width: 100, height: 80 },
    },
  );
  const second = add(first, "b", 90, 70),
    replay = add(add(undefined, "a", 100, 80), "b", 90, 70);
  assert.deepEqual([...second.regions], [...replay.regions]);
  const shrunk = add(second, "a", 40, 30);
  assert.deepEqual(shrunk.regions.get("a"), second.regions.get("a"));
  assert.deepEqual(shrunk.items.get("a"), { width: 40, height: 30 });
  valid(shrunk);
});

test("atlas relocation, removal, growth, and capacity failure remain atomic", () => {
  let layout = add(undefined, "a", 200, 200);
  layout = add(layout, "b", 200, 200);
  const before = [...layout.regions];
  const oversized = planAtlasUpsert(layout, { id: "bad", width: 8193, height: 1 });
  assert.deepEqual(oversized, { ok: false, code: "atlas.dimension" });
  assert.deepEqual([...layout.regions], before);
  const grown = add(layout, "a", 500, 300);
  assert(grown.width * grown.height >= layout.width * layout.height);
  valid(grown);
  const removed = removeAtlasItem(grown, "b")!;
  assert.equal(removed.items.has("b"), false);
  valid(removed);
  assert.equal(removeAtlasItem(removed, "a"), undefined);
});

test("atlas compaction is deferred unless occupancy and savings justify it", () => {
  let layout = add(undefined, "large", 1000, 1000);
  layout = add(layout, "small", 100, 100);
  layout = removeAtlasItem(layout, "large")!;
  const compact = planAtlasCompaction(layout);
  assert(compact?.ok);
  if (compact?.ok) {
    assert(compact.layout.width * compact.layout.height <= (layout.width * layout.height) / 2);
    valid(compact.layout);
  }
});
