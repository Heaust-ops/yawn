import { expect, test, type Page } from "@playwright/test";
import type { FxNode } from "@lib/index.js";
import type { GraphLayoutV2, GraphSnapshot } from "@lib/core/types.js";
import type { ColorRamp } from "@lib/widgets/color-ramp.js";

type Point = Readonly<{ x: number; y: number }>;
type Events = { mutations: number[]; snapshots: number[] };
type State = { snapshot: GraphSnapshot; layout: GraphLayoutV2; events: Events };
type RampWindow = Window &
  typeof globalThis & {
    rampTest: { root: FxNode | null; view: import("@lib/index.js").FxNodeView | null; ready: Promise<void> };
    rampEvents: Events;
  };

// The fixture ramp's compound world origin is frame (-300,250) + child (40,-40),
// hence (-260,210), width 320. At zoom 1 the canvas origin is (600,320).
// Layout's documented ramp bounds start at node.x+10 and node.y-header-2 rows-3:
// screen origin (350,185), width 300; rows are toolbar +0, menus +22,
// gradient +46, handles +74, and details +104.
const P = Object.freeze({
  add: { x: 368, y: 195 },
  remove: { x: 404, y: 195 },
  flip: { x: 479, y: 195 },
  distribute: { x: 593, y: 195 },
  mode: { x: 395, y: 217 },
  interpolation: { x: 501, y: 217 },
  hue: { x: 614, y: 217 },
  gradient: { x: 500, y: 242 },
  firstHandle: { x: 386, y: 271 },
  selector: { x: 387, y: 299 },
  position: { x: 483, y: 299 },
  swatch: { x: 380, y: 321 },
  r: { x: 440, y: 321 },
  g: { x: 500, y: 321 },
  b: { x: 560, y: 321 },
  a: { x: 620, y: 321 },
} satisfies Record<string, Point>);

async function open(page: Page) {
  await page.goto("/examples/blender/ramp-test/");
  await page.evaluate(() => (window as unknown as RampWindow).rampTest.ready);
  await page.evaluate(() => {
    const w = window as unknown as RampWindow;
    w.rampEvents = { mutations: [], snapshots: [] };
    w.rampTest.root!.onMutations((e) => w.rampEvents.mutations.push(e.version));
    w.rampTest.root!.onSnapshots((e) => w.rampEvents.snapshots.push(e.version));
  });
  return page.locator("#ramp");
}
const state = (page: Page): Promise<State> =>
  page.evaluate(async () => {
    const w = window as unknown as RampWindow;
    return {
      snapshot: await w.rampTest.root!.getState(),
      layout: await w.rampTest.root!.save(),
      events: structuredClone(w.rampEvents),
    };
  });
const ramp = (s: State): ColorRamp =>
  (s.layout.nodes.find((n) => n.id === "ramp")!.parameters.ramp as { readonly kind: "json"; readonly value: unknown })
    .value as ColorRamp;
function pairs(before: State, after: State, count: number) {
  const m = after.events.mutations.slice(before.events.mutations.length),
    s = after.events.snapshots.slice(before.events.snapshots.length);
  expect(m).toEqual(s);
  expect(m).toHaveLength(count);
  expect(after.snapshot.version - before.snapshot.version).toBe(count);
}
async function drag(page: Page, from: Point, dx: number, shift = false) {
  const box = await page.locator("#ramp").boundingBox();
  if (!box) throw new Error("Canvas missing");
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.move(box.x + from.x, box.y + from.y);
  await page.mouse.down();
  await page.mouse.move(box.x + from.x + dx, box.y + from.y, { steps: 5 });
  return async () => {
    await page.mouse.up();
    if (shift) await page.keyboard.up("Shift");
  };
}
const undo = (page: Page) => page.evaluate(() => (window as unknown as RampWindow).rampTest.root!.undo());

test("handle drag previews pixels, commits once, and Escape/RMB cancel", async ({ page }) => {
  const canvas = await open(page),
    a = await state(page),
    pixels = await canvas.screenshot(),
    release = await drag(page, P.firstHandle, 45);
  await expect.poll(async () => (await canvas.screenshot()).equals(pixels)).toBe(false);
  const preview = await state(page);
  expect(preview).toEqual(a);
  await release();
  const b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops.find((s) => s.id === "red")!.position).toBeGreaterThan(0.2);
  await page.evaluate(() => (window as unknown as RampWindow).rampTest.view!.whenRendered());
  const stopped = await canvas.screenshot();
  await page.mouse.move(700, 400);
  expect(await canvas.screenshot()).toEqual(stopped);
  const cancel = await drag(page, { ...P.firstHandle, x: 386 + 45 }, 30);
  await canvas.press("Escape");
  await cancel();
  pairs(b, await state(page), 0);
  const cancelRmb = await drag(page, { ...P.firstHandle, x: 386 + 45 }, -25);
  await page.mouse.down({ button: "right" });
  await page.mouse.up({ button: "right" });
  await cancelRmb();
  pairs(b, await state(page), 0);
});

test("plain gradient click inserts one transiently-active stop and undo removes it", async ({ page }) => {
  const canvas = await open(page),
    a = await state(page);
  await canvas.click({ position: P.gradient });
  const b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops).toHaveLength(4);
  expect(ramp(b).stops.filter((s) => !ramp(a).stops.some((old) => old.id === s.id))).toHaveLength(1);
  expect(JSON.stringify(b.layout)).not.toContain("activeRamp");
  expect(JSON.stringify(b.snapshot)).not.toContain("activeRamp");
  await undo(page);
  const c = await state(page);
  pairs(b, c, 1);
  expect(ramp(c)).toEqual(ramp(a));
});

test("transparent gradient checker is clipped to its exact bounds", async ({ page }) => {
  const canvas = await open(page);
  const pixels = await canvas.evaluate((element) => {
    const context = (element as HTMLCanvasElement).getContext("2d")!;
    return {
      x: Array.from(context.getImageData(643, 240, 1, 1).data),
      y: Array.from(context.getImageData(500, 260, 1, 1).data),
    };
  });
  for (const pixel of [pixels.x, pixels.y]) {
    expect(pixel[3]).toBe(255);
    expect(pixel.slice(0, 3)).not.toEqual([119, 119, 119]);
    expect(pixel.slice(0, 3)).not.toEqual([170, 170, 170]);
  }
});

test("toolbar add/remove commits atomically and enforces two stops; shortcuts stay scoped", async ({ page }) => {
  const canvas = await open(page),
    a = await state(page);
  await canvas.click({ position: P.firstHandle });
  await canvas.click({ position: P.add });
  const b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops).toHaveLength(4);
  await canvas.click({ position: P.remove });
  const c = await state(page);
  pairs(b, c, 1);
  expect(ramp(c).stops).toHaveLength(3);
  await canvas.press("Delete");
  const d = await state(page);
  pairs(c, d, 1);
  expect(ramp(d).stops).toHaveLength(2);
  await canvas.press("Delete");
  pairs(d, await state(page), 0);
  await canvas.press("g");
  await canvas.press("m");
  expect((await state(page)).layout.nodes).toHaveLength(2);
});

test("mode, interpolation, and hue controls cycle legal values and survive save/load", async ({ page }) => {
  const canvas = await open(page);
  let a = await state(page);
  const controls: readonly [Point, "colorMode" | "interpolation" | "hueInterpolation", readonly string[]][] = [
    [P.mode, "colorMode", ["rgb", "hsv", "hsl"]],
    [P.interpolation, "interpolation", ["linear", "ease", "constant", "cardinal", "b-spline"]],
    [P.hue, "hueInterpolation", ["near", "far", "clockwise", "counter-clockwise"]],
  ];
  for (const [point, key, legal] of controls) {
    await canvas.click({ position: point });
    const b = await state(page);
    pairs(a, b, 1);
    expect(legal).toContain(ramp(b)[key]);
    a = b;
  }
  const saved = a.layout,
    persisted = ramp(a);
  await canvas.click({ position: P.mode });
  const changed = await state(page);
  await page.evaluate(
    (layout: unknown) => (window as unknown as RampWindow).rampTest.root!.load(layout),
    saved as unknown,
  );
  const loaded = await state(page);
  pairs(changed, loaded, 1);
  expect(ramp(loaded)).toEqual(persisted);
});

test("position scrub alters only the active stop position", async ({ page }) => {
  await open(page);
  let a = await state(page);
  const finishPos = await drag(page, P.position, 10, true);
  await finishPos();
  let b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops[0]!.position).toBeCloseTo(ramp(a).stops[0]!.position + 0.01);
  expect(ramp(b).stops[0]!.color).toEqual(ramp(a).stops[0]!.color);
});

test("active stop swatch opens an Oklch picker with transient preview and atomic commit", async ({ page }) => {
  const canvas = await open(page),
    before = await state(page);
  await canvas.click({ position: P.swatch });
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Canvas missing");
  await page.mouse.move(box.x + 756, box.y + 409);
  await page.mouse.down();
  await page.mouse.move(box.x + 816, box.y + 409, { steps: 4 });
  pairs(before, await state(page), 0);
  await page.mouse.up();
  pairs(before, await state(page), 0);
  await canvas.click({ position: { x: 676, y: 322 } });
  const committed = await state(page);
  pairs(before, committed, 1);
  expect(ramp(committed).stops[0]!.color).not.toEqual(ramp(before).stops[0]!.color);
  await canvas.click({ position: P.swatch });
  await page.mouse.move(box.x + 756, box.y + 409);
  await page.mouse.down();
  await page.mouse.move(box.x + 776, box.y + 372);
  await page.mouse.up();
  await canvas.press("Escape");
  pairs(committed, await state(page), 0);
});

test("Backspace resets ramp or active color, undo restores, and Escape is inert", async ({ page }) => {
  const canvas = await open(page),
    a = await state(page);
  await canvas.click({ position: P.position });
  await canvas.press("Backspace");
  const reset = await state(page);
  pairs(a, reset, 1);
  expect(ramp(reset).stops.map((s) => s.position)).toEqual([0, 1]);
  await undo(page);
  const restored = await state(page);
  pairs(reset, restored, 1);
  expect(ramp(restored)).toEqual(ramp(a));
  await canvas.hover({ position: P.swatch });
  await canvas.press("Backspace");
  const black = await state(page);
  pairs(restored, black, 1);
  expect(ramp(black).stops[0]!.color).toEqual([0, 0, 0, 1]);
  await canvas.press("Escape");
  pairs(black, await state(page), 0);
});

test("flip and distribute are one-commit, one-step-undo tools", async ({ page }) => {
  const canvas = await open(page);
  let a = await state(page);
  await canvas.click({ position: P.flip });
  let b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops.map((s) => s.position)).toEqual([
    expect.closeTo(0.1),
    expect.closeTo(0.65),
    expect.closeTo(0.9),
  ]);
  await undo(page);
  let c = await state(page);
  pairs(b, c, 1);
  expect(ramp(c)).toEqual(ramp(a));
  a = c;
  await canvas.click({ position: P.distribute });
  b = await state(page);
  pairs(a, b, 1);
  expect(ramp(b).stops.map((s) => s.position)).toEqual([0.1, 0.5, 0.9]);
  await undo(page);
  c = await state(page);
  pairs(b, c, 1);
  expect(ramp(c)).toEqual(ramp(a));
});
