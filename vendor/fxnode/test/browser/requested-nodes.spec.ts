import { expect, test, type Page } from "@playwright/test";

type Point = Readonly<{ x: number; y: number }>;
type Saved = Awaited<ReturnType<typeof saved>>;

// The parity page is deliberately used instead of a test-only worker hook.  At
// 1280x1200, zoom one maps world (0,0) to canvas (640,600).  These are centers
// of public canvas controls derived from the descriptor layout's 24px rows.
const P = Object.freeze({
  image: {
    resource: { x: 260, y: 525 },
    interpolation: { x: 312, y: 552 },
    projection: { x: 312, y: 576 },
    flatExtension: { x: 312, y: 600 },
    boxBlend: { x: 312, y: 600 },
    boxExtension: { x: 312, y: 624 },
  },
  compositor: {
    resource: { x: 420, y: 965 },
    source: { x: 464, y: 992 },
    frames: { x: 464, y: 1016 },
    start: { x: 464, y: 1040 },
    offset: { x: 464, y: 1064 },
    cyclic: { x: 464, y: 1088 },
    refresh: { x: 464, y: 1112 },
  },
  noise: { dimensions: { x: 653, y: 456 }, type: { x: 653, y: 480 } },
  master: {
    mode: { x: 961, y: 896 },
    scalar: { x: 729, y: 1048 },
    liftWheel: { x: 729, y: 990 },
    whiteTemperature: { x: 961, y: 944 },
    eye: { x: 860, y: 1016 },
  },
});

async function open(page: Page) {
  await page.setViewportSize({ width: 1280, height: 1200 });
  await page.goto("/examples/blender/parity/");
  await page.waitForFunction(() => !!window.parityExample);
  await page.evaluate(() => {
    const w = window as typeof window & { requestedEvents: { m: number[]; s: number[] } };
    w.requestedEvents = { m: [], s: [] };
    window.parityExample!.root.onMutations((e) => w.requestedEvents.m.push(e.version));
    window.parityExample!.root.onMutations(() => {
      throw new Error("intentional requested-node subscriber failure");
    });
    window.parityExample!.root.onSnapshots((e) => w.requestedEvents.s.push(e.version));
  });
  return page.locator("#graph");
}
async function saved(page: Page) {
  return page.evaluate(async () => ({
    layout: await window.parityExample!.root.save(),
    snapshot: await window.parityExample!.root.getState(),
    events: (window as typeof window & { requestedEvents: { m: number[]; s: number[] } }).requestedEvents,
  }));
}
const node = (state: Saved, id: string) => state.layout.nodes.find((n) => n.id === id)!;
const parameter = (state: Saved, id: string, key: string) => node(state, id).parameters[key]!;
function paired(a: Saved, b: Saved, count = 1) {
  expect(b.snapshot.version - a.snapshot.version).toBe(count);
  expect(b.events.m.slice(a.events.m.length)).toEqual(b.events.s.slice(a.events.s.length));
  expect(b.events.m.length - a.events.m.length).toBe(count);
}
async function pixels(page: Page, x: number, y: number, w: number, h: number) {
  return page.locator("#graph").evaluate(
    (c, r) => {
      const d = (c as HTMLCanvasElement).getContext("2d")!.getImageData(r.x, r.y, r.w, r.h).data;
      let hash = 2166136261;
      for (const v of d) hash = Math.imul(hash ^ v, 16777619);
      return hash >>> 0;
    },
    { x, y, w, h },
  );
}
async function scrub(page: Page, p: Point, dx = 25) {
  const box = await page.locator("#graph").boundingBox();
  if (!box) throw Error("canvas bounds missing");
  await page.mouse.move(box.x + p.x, box.y + p.y);
  await page.mouse.down();
  await page.mouse.move(box.x + p.x + dx, box.y + p.y, { steps: 4 });
  await page.mouse.up();
}
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACAQMAAABIeJ9nAAAABGdBTUEAALGPC/xhBQAAACBjSFJNAAB6JgAAgIQAAPoAAACA6AAAdTAAAOpgAAA6mAAAF3CculE8AAAABlBMVEUzZpn////xDcYdAAAAAWJLR0QB/wIt3gAAAAd0SU1FB+oHFQw4FzO6KaEAAAAMSURBVAjXY2BgYAAAAAQAASc0JwoAAAAldEVYdGRhdGU6Y3JlYXRlADIwMjYtMDctMjFUMTI6NTY6MjMrMDA6MDC80CO3AAAAJXRFWHRkYXRlOm1vZGlmeQAyMDI2LTA3LTIxVDEyOjU2OjIzKzAwOjAwzY2bCwAAAABJRU5ErkJggg==",
  "base64",
);
async function chooseImage(page: Page, canvas: ReturnType<Page["locator"]>, position: Point, name: string) {
  const pending = page.waitForEvent("filechooser");
  await canvas.click({ position });
  const chooser = await pending;
  await chooser.setFiles({ name, mimeType: "image/png", buffer: PNG });
}

test("Image Texture conditional blend and persisted texture modes", async ({ page }) => {
  const canvas = await open(page);
  let a = await saved(page);
  const emptyPreview = await pixels(page, 130, 448, 260, 62);
  await chooseImage(page, canvas, P.image.resource, "texture.png");
  await expect
    .poll(async () => (parameter(await saved(page), "image-texture", "image") as { value: string }).value)
    .toContain("texture.png");
  await page.evaluate(() => window.parityExample!.view.whenRendered());
  const uploaded = await saved(page);
  paired(a, uploaded);
  expect(await pixels(page, 130, 448, 260, 62)).not.toBe(emptyPreview);
  a = uploaded;
  const flatPixels = await pixels(page, 110, 410, 300, 300);
  expect(node(a, "image-texture").sockets.map((s) => [s.key, s.direction, s.dataType])).toEqual([
    ["vector", "input", "vector"],
    ["color", "output", "color"],
    ["alpha", "output", "float"],
  ]);
  await canvas.click({ position: P.image.projection });
  await page.evaluate(() => window.parityExample!.view.whenRendered());
  const box = await saved(page);
  paired(a, box);
  expect(parameter(box, "image-texture", "projection")).toEqual({ kind: "string", value: "Box" });
  expect(await pixels(page, 110, 410, 300, 300)).not.toBe(flatPixels);
  await scrub(page, P.image.boxBlend);
  const blended = await saved(page);
  paired(box, blended);
  expect((parameter(blended, "image-texture", "blend") as { value: number }).value).toBeGreaterThan(0);
  await canvas.hover({ position: P.image.boxBlend });
  await canvas.press("Backspace");
  const reset = await saved(page);
  paired(blended, reset);
  expect(parameter(reset, "image-texture", "blend")).toEqual({ kind: "number", value: 0 });
  await scrub(page, P.image.boxBlend, 15);
  await canvas.click({ position: P.image.projection });
  const sphere = await saved(page);
  expect(parameter(sphere, "image-texture", "projection")).toEqual({ kind: "string", value: "Sphere" });
  expect((parameter(sphere, "image-texture", "blend") as { value: number }).value).toBeGreaterThan(0);
  // The old Blend coordinate is now Extension: hit behavior proves that no
  // Blend control remains there, while the saved Blend value stays authored.
  await canvas.click({ position: P.image.boxBlend });
  let hidden = await saved(page);
  expect(parameter(hidden, "image-texture", "extension")).toEqual({ kind: "string", value: "Extend" });
  expect((parameter(hidden, "image-texture", "blend") as { value: number }).value).toBeGreaterThan(0);
  await canvas.click({ position: P.image.interpolation });
  const changed = await saved(page);
  expect(parameter(changed, "image-texture", "interpolation")).toEqual({ kind: "string", value: "Closest" });
  expect(parameter(changed, "image-texture", "extension")).toEqual({ kind: "string", value: "Extend" });
  await page.evaluate(async () => {
    const x = await window.parityExample!.root.save();
    await window.parityExample!.root.load(x);
  });
  const loaded = await saved(page);
  expect(node(loaded, "image-texture").parameters).toEqual(node(changed, "image-texture").parameters);
});

test("Compositor Image movie rows, resource editing and output contract", async ({ page }) => {
  const canvas = await open(page),
    a = await saved(page);
  const n = node(a, "compositor-image");
  expect(n.sockets.map((s) => [s.key, s.direction, s.dataType])).toEqual([
    ["image", "output", "color"],
    ["alpha", "output", "float"],
    ["z", "output", "float"],
  ]);
  expect(Object.keys(n.parameters)).not.toEqual(expect.arrayContaining(["projection", "interpolation", "vector"]));
  await chooseImage(page, canvas, P.compositor.resource, "plate.png");
  await expect
    .poll(async () => (parameter(await saved(page), "compositor-image", "image") as { value: string }).value)
    .toContain("plate.png");
  let b = await saved(page);
  paired(a, b);
  await canvas.click({ position: P.compositor.source });
  b = await saved(page);
  expect(parameter(b, "compositor-image", "source")).toEqual({ kind: "string", value: "Movie" });
  await scrub(page, P.compositor.frames);
  await scrub(page, P.compositor.start);
  await scrub(page, P.compositor.offset);
  await canvas.click({ position: P.compositor.cyclic });
  await canvas.click({ position: P.compositor.refresh });
  const edited = await saved(page);
  expect((parameter(edited, "compositor-image", "frames") as { value: number }).value).toBeGreaterThan(1);
  await canvas.hover({ position: { x: 10, y: 10 } });
  await canvas.hover({ position: P.compositor.frames });
  await page.keyboard.press("Backspace");
  expect(parameter(await saved(page), "compositor-image", "frames")).toEqual({ kind: "number", value: 1 });
  // Movie -> Sequence retains the same rows; cycling through Multilayer,
  // Generated and File hides them without deleting their authored values.
  await canvas.click({ position: P.compositor.source });
  expect(parameter(await saved(page), "compositor-image", "source")).toEqual({ kind: "string", value: "Sequence" });
  for (let i = 0; i < 3; i++) {
    await canvas.click({ position: P.compositor.source });
    await page.evaluate(() => window.parityExample!.view.whenRendered());
  }
  const file = await saved(page);
  expect(parameter(file, "compositor-image", "source")).toEqual({ kind: "string", value: "File" });
  expect(parameter(file, "compositor-image", "offset")).toEqual(parameter(edited, "compositor-image", "offset"));
  const v = file.snapshot.version;
  await canvas.click({ position: P.compositor.frames });
  expect((await saved(page)).snapshot.version).toBe(v);
});

test("Noise dimensions/types use conditional hit rows and preserve hidden values", async ({ page }) => {
  const canvas = await open(page);
  let prior = await saved(page),
    hash = await pixels(page, 410, 410, 220, 380);
  // 3D -> 4D -> 1D, then 1D -> 2D -> 3D -> 4D (actual enum events).
  await canvas.click({ position: P.noise.dimensions });
  await canvas.click({ position: P.noise.dimensions });
  let one = await saved(page);
  paired(prior, one, 2);
  expect(parameter(one, "noise-3d", "dimensions")).toEqual({ kind: "string", value: "1d" });
  expect(await pixels(page, 410, 410, 220, 380)).not.toBe(hash);
  // W is the first socket row in 1D (y=528).
  await scrub(page, { x: 653, y: 528 });
  const wEdited = await saved(page);
  expect(
    (node(wEdited, "noise-3d").sockets.find((s) => s.key === "w")!.defaultValue as { value: number }).value,
  ).toBeGreaterThan(0);
  for (let i = 0; i < 3; i++) await canvas.click({ position: P.noise.dimensions });
  let four = await saved(page);
  expect(parameter(four, "noise-3d", "dimensions")).toEqual({ kind: "string", value: "4d" });
  await canvas.click({ position: P.noise.type });
  await canvas.click({ position: P.noise.type });
  let hybrid = await saved(page);
  expect(parameter(hybrid, "noise-3d", "noiseType")).toEqual({ kind: "string", value: "hybrid-multifractal" });
  // In 4D hybrid: vector,w,scale,detail,roughness,lacunarity,offset,gain.
  await scrub(page, { x: 653, y: 648 });
  await scrub(page, { x: 653, y: 672 });
  const conditional = await saved(page);
  for (const key of ["offset", "gain"])
    expect(
      (node(conditional, "noise-3d").sockets.find((s) => s.key === key)!.defaultValue as { value: number }).value,
    ).toBeGreaterThan(0);
  await canvas.click({ position: P.noise.type });
  expect(parameter(await saved(page), "noise-3d", "noiseType")).toEqual({
    kind: "string",
    value: "ridged-multifractal",
  });
  await canvas.click({ position: P.noise.type });
  const hetero = await saved(page);
  expect(parameter(hetero, "noise-3d", "noiseType")).toEqual({ kind: "string", value: "hetero-terrain" });
  expect(node(hetero, "noise-3d").sockets.find((s) => s.key === "gain")!.defaultValue).toEqual(
    node(conditional, "noise-3d").sockets.find((s) => s.key === "gain")!.defaultValue,
  );
  expect(parameter(hetero, "noise-3d", "normalize")).toEqual({ kind: "boolean", value: false });
  await canvas.hover({ position: P.noise.type });
  await canvas.press("Backspace");
  await page.evaluate(() => window.parityExample!.view.whenRendered());
  await canvas.hover({ position: { x: 10, y: 10 } });
  await canvas.hover({ position: P.noise.dimensions });
  await canvas.press("Backspace");
  const reset = await saved(page);
  expect(parameter(reset, "noise-3d", "noiseType")).toEqual({ kind: "string", value: "fbm" });
  expect(parameter(reset, "noise-3d", "dimensions")).toEqual({ kind: "string", value: "3d" });
});

test("Master Color Grading modes edit, persist, load and undo", async ({ page }) => {
  const canvas = await open(page),
    initial = await saved(page);
  expect(node(initial, "master").label).toBe("Master Color Grading");
  const lgg = await pixels(page, 650, 850, 420, 350);
  await scrub(page, P.master.scalar);
  const box = await canvas.boundingBox();
  if (!box) throw Error("canvas bounds missing");
  await page.mouse.move(box.x + P.master.liftWheel.x, box.y + P.master.liftWheel.y);
  await page.mouse.down();
  await page.mouse.move(box.x + P.master.liftWheel.x + 25, box.y + P.master.liftWheel.y, { steps: 4 });
  paired(initial, await saved(page), 1);
  await page.mouse.up();
  const lift = await saved(page);
  paired(initial, lift, 2);
  expect((parameter(lift, "master", "lift") as { value: number }).value).toBeGreaterThan(0);
  expect(parameter(lift, "master", "liftColor")).not.toEqual(parameter(initial, "master", "liftColor"));
  await canvas.click({ position: P.master.mode });
  let ops = await saved(page);
  expect(parameter(ops, "master", "mode")).toEqual({ kind: "string", value: "Offset/Power/Slope" });
  expect(await pixels(page, 650, 850, 420, 350)).not.toBe(lgg);
  await scrub(page, P.master.scalar);
  ops = await saved(page);
  expect((parameter(ops, "master", "offset") as { value: number }).value).toBeGreaterThan(0);
  expect(parameter(ops, "master", "lift")).toEqual(parameter(lift, "master", "lift"));
  await canvas.click({ position: P.master.mode });
  let white = await saved(page);
  expect(parameter(white, "master", "mode")).toEqual({ kind: "string", value: "White Point" });
  await scrub(page, P.master.whiteTemperature);
  white = await saved(page);
  expect((parameter(white, "master", "inputTemperature") as { value: number }).value).toBeGreaterThan(6500);
  const beforeEye = white.snapshot.version;
  await canvas.click({ position: P.master.eye });
  expect((await saved(page)).snapshot.version).toBe(beforeEye);
  const persisted = await page.evaluate(async () => {
    const x = await window.parityExample!.root.save();
    await window.parityExample!.root.load(x);
    return window.parityExample!.root.save();
  });
  expect(persisted.nodes.find((n) => n.id === "master")!.label).toBe("Master Color Grading");
  expect(persisted.nodes.find((n) => n.id === "master")!.parameters).toEqual(node(white, "master").parameters);
  await canvas.click({ position: P.master.mode });
  expect(parameter(await saved(page), "master", "mode")).toEqual({ kind: "string", value: "Lift/Gamma/Gain" });
  await page.evaluate(() => window.parityExample!.root.undo());
  expect(parameter(await saved(page), "master", "mode")).toEqual({ kind: "string", value: "White Point" });
});
