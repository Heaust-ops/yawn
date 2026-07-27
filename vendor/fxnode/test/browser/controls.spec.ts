import { expect, test, type Page } from "@playwright/test";

type Point = Readonly<{ x: number; y: number }>;
type ControlPage = Page & { readonly __controlPageBrand?: never };

// Canvas is 1200x640 and the zoom-1 origin is its center (600,320).
// Layout rows are 24px high below a 24px header. Scalar numeric fields span
// nearly the full row; compound and non-numeric controls retain split layout.
const CONTROL_COORDS = Object.freeze({
  value: { x: 196, y: 147 },
  mathOperation: { x: 473, y: 147 },
  mathClamp: { x: 473, y: 171 },
  mathA: { x: 473, y: 195 },
  mathB: { x: 473, y: 219 },
  vectorOperation: { x: 853, y: 147 },
  vectorAX: { x: 792, y: 171 },
  vectorAY: { x: 853, y: 171 },
  colorSwatch: { x: 1003, y: 147 },
  groupString: { x: 623, y: 456 },
} satisfies Record<string, Point>);

const open = async (page: Page): Promise<ControlPage> => {
  await page.goto("/examples/blender/control-test/");
  await page.evaluate(() => window.controlTest.ready);
  expect(await page.locator('input[type="file"]').count()).toBe(0);
  await page.evaluate(() => {
    window.controlEvents = { mutations: [], snapshots: [] };
    window.controlTest.root!.onMutations((event) => window.controlEvents.mutations.push(event.version));
    window.controlTest.root!.onSnapshots((event) => window.controlEvents.snapshots.push(event.version));
  });
  return page;
};
const state = (page: ControlPage) =>
  page.evaluate(async () => ({
    snapshot: await window.controlTest.root!.getState(),
    layout: await window.controlTest.root!.save(),
    events: window.controlEvents,
  }));
const value = (layout: Awaited<ReturnType<typeof state>>["layout"], id: string, parameter: string) =>
  layout.nodes.find((node) => node.id === id)!.parameters[parameter]!;
const socketValue = (layout: Awaited<ReturnType<typeof state>>["layout"], id: string, socket: string) =>
  layout.nodes.find((node) => node.id === id)!.sockets.find((item) => item.id === socket)!.defaultValue!;
const invariant = (
  before: Awaited<ReturnType<typeof state>>,
  after: Awaited<ReturnType<typeof state>>,
  commits: number,
) => {
  expect(after.snapshot.version - before.snapshot.version).toBe(commits);
  expect(after.events.mutations.slice(before.events.mutations.length)).toEqual(
    after.events.snapshots.slice(before.events.snapshots.length),
  );
  expect(after.events.mutations.length - before.events.mutations.length).toBe(commits);
};
async function scrub(
  page: ControlPage,
  point: Point,
  dx: number,
  modifiers: { shift?: boolean; control?: boolean } = {},
) {
  const canvas = page.locator("#controls"),
    box = await canvas.boundingBox();
  if (!box) throw new Error("Canvas bounds missing");
  if (modifiers.shift) await page.keyboard.down("Shift");
  if (modifiers.control) await page.keyboard.down("Control");
  await page.mouse.move(box.x + point.x, box.y + point.y);
  await page.mouse.down();
  await page.mouse.move(box.x + point.x + dx / 2, box.y + point.y, { steps: 3 });
  await page.mouse.move(box.x + point.x + dx, box.y + point.y, { steps: 3 });
  return async () => {
    await page.mouse.up();
    if (modifiers.control) await page.keyboard.up("Control");
    if (modifiers.shift) await page.keyboard.up("Shift");
  };
}

test("number scrub is transient, atomic, cancellable, precise and snapping", async ({ page }) => {
  const p = await open(page),
    initial = await state(p),
    release = await scrub(p, CONTROL_COORDS.value, 30);
  const preview = await state(p);
  expect(preview.snapshot.version).toBe(initial.snapshot.version);
  expect(preview.events).toEqual(initial.events);
  await release();
  const normal = await state(p);
  invariant(initial, normal, 1);
  expect(value(normal.layout, "value", "value")).toEqual({ kind: "number", value: 3 });
  const cancelStart = await state(p),
    cancel = await scrub(p, CONTROL_COORDS.value, 30);
  await p.locator("#controls").press("Escape");
  await cancel();
  invariant(cancelStart, await state(p), 0);
  const shiftStart = await state(p),
    shiftedRelease = await scrub(p, CONTROL_COORDS.value, 30, { shift: true });
  await shiftedRelease();
  const shifted = await state(p);
  invariant(shiftStart, shifted, 1);
  expect((value(shifted.layout, "value", "value") as { value: number }).value - 3).toBeLessThan(3);
  const ctrlStart = await state(p),
    ctrlRelease = await scrub(p, CONTROL_COORDS.value, 37, { control: true });
  await ctrlRelease();
  const snapped = await state(p);
  invariant(ctrlStart, snapped, 1);
  expect(Number.isInteger((value(snapped.layout, "value", "value") as { value: number }).value)).toBe(true);
});

test("numeric fields support typed assignment, cancellation, clamping and step arrows", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    a = await state(p);
  await canvas.click({ position: CONTROL_COORDS.value });
  await p.keyboard.type("0.375");
  await p.keyboard.press("Enter");
  const b = await state(p);
  invariant(a, b, 1);
  expect(value(b.layout, "value", "value")).toEqual({ kind: "number", value: 0.375 });
  await canvas.click({ position: CONTROL_COORDS.value });
  await p.keyboard.type("999");
  await p.keyboard.press("Enter");
  const clamped = await state(p);
  invariant(b, clamped, 1);
  expect(value(clamped.layout, "value", "value")).toEqual({ kind: "number", value: 999 });
  await canvas.click({ position: CONTROL_COORDS.value });
  await p.keyboard.type("bad");
  await p.keyboard.press("Enter");
  invariant(clamped, await state(p), 0);
  await p.keyboard.press("Escape");
  await canvas.click({ position: { x: 235, y: 147 } });
  const stepped = await state(p);
  invariant(clamped, stepped, 1);
  expect(value(stepped.layout, "value", "value")).toEqual({ kind: "number", value: 1000 });
  await canvas.click({ position: { x: 115, y: 147 } });
  const decremented = await state(p);
  invariant(stepped, decremented, 1);
  expect(value(decremented.layout, "value", "value")).toEqual({ kind: "number", value: 999 });
});

test("vector component scrubs commit only the selected component", async ({ page }) => {
  const p = await open(page),
    a = await state(p),
    r1 = await scrub(p, CONTROL_COORDS.vectorAY, 20);
  await r1();
  const b = await state(p);
  invariant(a, b, 1);
  expect(socketValue(b.layout, "vector", "vector:a")).toEqual({ kind: "vector", value: [0, 2, 0] });
});

test("color picker keeps RGBA, HSV and hex previews transient until confirmation", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    before = await state(p);
  await canvas.click({ position: CONTROL_COORDS.colorSwatch });
  const box = await canvas.boundingBox();
  if (!box) throw new Error("Canvas bounds missing");
  await page.mouse.move(box.x + 805, box.y + 245);
  await page.mouse.down();
  await page.mouse.move(box.x + 850, box.y + 245, { steps: 4 });
  await page.mouse.up();
  invariant(before, await state(p), 0);
  for (const [field, text] of [
    [{ x: 742, y: 378 }, "0.25"],
    [{ x: 752, y: 408 }, "120"],
    [{ x: 830, y: 439 }, "#336699CC"],
  ] as const) {
    await canvas.click({ position: field });
    await page.keyboard.type(text);
    await page.keyboard.press("Enter");
    invariant(before, await state(p), 0);
  }
  await canvas.click({ position: { x: 723, y: 163 } });
  const committed = await state(p);
  invariant(before, committed, 1);
  expect(value(committed.layout, "color", "color")).toEqual({ kind: "color", value: [0.2, 0.4, 0.6, 0.8] });
  await canvas.click({ position: CONTROL_COORDS.colorSwatch });
  await canvas.click({ position: { x: 830, y: 439 } });
  await page.keyboard.type("#FF0000");
  await canvas.click({ position: { x: 20, y: 20 } });
  const outside = await state(p);
  invariant(committed, outside, 1);
  expect(value(outside.layout, "color", "color")).toEqual({ kind: "color", value: [1, 0, 0, 1] });
  await canvas.click({ position: CONTROL_COORDS.colorSwatch });
  await page.mouse.move(box.x + 805, box.y + 245);
  await page.mouse.down();
  await page.mouse.move(box.x + 825, box.y + 220);
  await page.mouse.up();
  await canvas.press("Escape");
  invariant(outside, await state(p), 0);
});

test("enum and boolean controls emit one mutation/snapshot pair", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    a = await state(p);
  await canvas.click({ position: CONTROL_COORDS.mathOperation });
  const b = await state(p);
  invariant(a, b, 1);
  expect(value(b.layout, "math", "operation")).toEqual({ kind: "string", value: "subtract" });
  await canvas.press("ArrowDown");
  const c = await state(p);
  invariant(b, c, 1);
  expect(value(c.layout, "math", "operation")).toEqual({ kind: "string", value: "multiply" });
  await canvas.click({ position: CONTROL_COORDS.mathClamp });
  const d = await state(p);
  invariant(c, d, 1);
  expect(value(d.layout, "math", "clamp")).toEqual({ kind: "boolean", value: true });
});

test("string editing commits on Enter and cancels on Escape or blur", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    a = await state(p);
  await canvas.click({ position: CONTROL_COORDS.groupString });
  await p.keyboard.type(" Name");
  await p.keyboard.press("Enter");
  const b = await state(p);
  invariant(a, b, 1);
  expect(value(b.layout, "group", "interfaceName")).toEqual({ kind: "string", value: "Socket Name" });
  await canvas.click({ position: CONTROL_COORDS.groupString });
  await p.keyboard.type(" bad");
  await p.keyboard.press("Escape");
  invariant(b, await state(p), 0);
  await canvas.click({ position: CONTROL_COORDS.groupString });
  await p.keyboard.type(" worse");
  await p.locator("body").evaluate((body) => (body as HTMLElement).focus());
  await canvas.evaluate((element) => (element as HTMLCanvasElement).blur());
  invariant(b, await state(p), 0);
});

test("Backspace resets defaults, undo restores, and linked socket is inert", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    a = await state(p),
    r = await scrub(p, CONTROL_COORDS.value, 30);
  await r();
  const changed = await state(p);
  invariant(a, changed, 1);
  await canvas.hover({ position: CONTROL_COORDS.value });
  await canvas.press("Backspace");
  const reset = await state(p);
  invariant(changed, reset, 1);
  expect(value(reset.layout, "value", "value")).toEqual({ kind: "number", value: 0 });
  await p.evaluate(() => window.controlTest.root!.undo());
  const undone = await state(p);
  invariant(reset, undone, 1);
  expect(value(undone.layout, "value", "value")).toEqual({ kind: "number", value: 3 });
  await canvas.hover({ position: CONTROL_COORDS.mathA });
  await canvas.press("Backspace");
  invariant(undone, await state(p), 0);
});

test("muting a link reveals its default control and unmuting hides it", async ({ page }) => {
  const p = await open(page),
    canvas = p.locator("#controls"),
    a = await state(p);
  await p.evaluate(() => {
    const id = window.controlTest.root!.save().then((layout) => layout.links[0]!.id);
    return id.then((link) => window.controlTest.root!.dispatch({ type: "link.mute", id: link, value: true }));
  });
  await p.evaluate(() => window.controlTest.view!.whenRendered());
  const muted = await state(p);
  invariant(a, muted, 1);
  expect(muted.layout.links[0]!.muted).toBe(true);
  const r = await scrub(p, CONTROL_COORDS.mathA, 20);
  await r();
  const edited = await state(p);
  invariant(muted, edited, 1);
  expect(socketValue(edited.layout, "math", "math:a")).toEqual({ kind: "number", value: 2 });
  await p.evaluate(() => {
    const id = window.controlTest.root!.save().then((layout) => layout.links[0]!.id);
    return id.then((link) => window.controlTest.root!.dispatch({ type: "link.mute", id: link, value: false }));
  });
  await p.evaluate(() => window.controlTest.view!.whenRendered());
  const live = await state(p);
  invariant(edited, live, 1);
  await canvas.click({ position: CONTROL_COORDS.mathA });
  invariant(live, await state(p), 0);
  expect((await state(p)).layout.links[0]!.muted).toBe(false);
});
