import { expect, test } from "@playwright/test";

test("M muting is one paired gesture for bypass nodes and generators", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph");
  await page.evaluate(() => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!;
    const state = { mutations: [] as number[], snapshots: [] as number[] };
    (window as typeof window & { muteEvents: typeof state }).muteEvents = state;
    // Subscriber exceptions are deliberately isolated from subsequent listeners.
    api.onMutations(() => {
      throw new Error("intentional test subscriber");
    });
    api.onMutations((event) => state.mutations.push(event.version));
    api.onSnapshots((event) => state.snapshots.push(event.version));
  });
  const baseline = (await page.evaluate(() => window.fxnodeExample.root!.getState())).version;

  // Fixture coordinates use the documented 1200x640 viewport and world origin
  // (600,320). Header centers are stable layout data, not worker introspection.
  await canvas.click({ position: { x: 380, y: 160 } }); // Math (-300,170), width 160
  await canvas.press("m");
  let snapshot = await page.evaluate(() => window.fxnodeExample.root!.getState());
  expect(snapshot.nodes.find((node) => node.id === "math")?.muted).toBe(true);
  expect(
    await page.evaluate(
      () => (window as typeof window & { muteEvents: { mutations: number[]; snapshots: number[] } }).muteEvents,
    ),
  ).toEqual({ mutations: [baseline + 1], snapshots: [baseline + 1] });

  await canvas.press("m");
  snapshot = await page.evaluate(() => window.fxnodeExample.root!.getState());
  expect(snapshot.nodes.find((node) => node.id === "math")?.muted).toBe(false);
  expect(
    await page.evaluate(
      () => (window as typeof window & { muteEvents: { mutations: number[]; snapshots: number[] } }).muteEvents,
    ),
  ).toEqual({ mutations: [baseline + 1, baseline + 2], snapshots: [baseline + 1, baseline + 2] });

  await canvas.click({ position: { x: 580, y: 140 } }); // Noise generator (-100,190)
  await canvas.press("m");
  snapshot = await page.evaluate(() => window.fxnodeExample.root!.getState());
  expect(snapshot.nodes.find((node) => node.id === "noise")?.muted).toBe(true);
  expect(snapshot.version).toBe(baseline + 3);
  expect(
    await page.evaluate(
      () => (window as typeof window & { muteEvents: { mutations: number[]; snapshots: number[] } }).muteEvents,
    ),
  ).toEqual({
    mutations: [baseline + 1, baseline + 2, baseline + 3],
    snapshots: [baseline + 1, baseline + 2, baseline + 3],
  });
  await canvas.press("Control+z");
  snapshot = await page.evaluate(() => window.fxnodeExample.root!.getState());
  expect(snapshot.nodes.find((node) => node.id === "noise")?.muted).toBe(false);
  expect(snapshot.nodes.find((node) => node.id === "math")?.muted).toBe(false);
});

test("equivalent selection summaries retain identity and API mute is one-step undoable", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph");
  expect(
    await page.evaluate(async () => {
      const api = window.fxnodeExample.root!,
        view = window.fxnodeExample.view!;
      await view.addNode({ typeId: "fxnode.shader.value", viewPosition: { x: 900, y: 500 }, nodeId: "summary-a" });
      const first = view.getHostSnapshot().selection;
      await view.addNode({ typeId: "fxnode.shader.value", viewPosition: { x: 1100, y: 500 }, nodeId: "summary-b" });
      return view.getHostSnapshot().selection === first;
    }),
  ).toBe(true);
  await page.evaluate(() =>
    window.fxnodeExample.root!.dispatch({ type: "node.mute", id: "summary-b" as never, value: true }),
  );
  expect((await page.evaluate(() => window.fxnodeExample.view!.getHostSnapshot())).selection.mute).toEqual({
    enabled: true,
    state: "all-muted",
  });
  const result = await page.evaluate(async () => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      order: string[] = [];
    api.onMutations((event) => order.push(`mutation:${event.version}`));
    api.onSnapshots((event) => order.push(`snapshot:${event.version}`));
    const receipt = await view.setSelectedMuted(false);
    order.push(`receipt:${receipt.version}`);
    const unmuted = structuredClone(view.getHostSnapshot().selection.mute),
      undo = await api.undo(),
      muted = structuredClone(view.getHostSnapshot().selection.mute),
      redo = await api.redo(),
      again = structuredClone(view.getHostSnapshot().selection.mute);
    return { receipt, unmuted, undo, muted, redo, again, order };
  });
  expect(result.unmuted).toEqual({ enabled: true, state: "all-unmuted" });
  expect(result.muted).toEqual({ enabled: true, state: "all-muted" });
  expect(result.again).toEqual({ enabled: true, state: "all-unmuted" });
  expect(result.receipt.status).toBe("committed");
  expect(result.undo.status).toBe("committed");
  expect(result.redo.status).toBe("committed");
  expect(result.order).toEqual([
    `mutation:${result.receipt.version}`,
    `snapshot:${result.receipt.version}`,
    `receipt:${result.receipt.version}`,
    `mutation:${result.undo.version}`,
    `snapshot:${result.undo.version}`,
    `mutation:${result.redo.version}`,
    `snapshot:${result.redo.version}`,
  ]);
});

test("collapse chevron animates for click, H, API, and interrupted reversal then idles", async ({ page }) => {
  await page.addInitScript(() => {
    let frames = 0;
    const original = CanvasRenderingContext2D.prototype.drawImage;
    Object.defineProperty(CanvasRenderingContext2D.prototype, "drawImage", {
      value: function (this: CanvasRenderingContext2D, ...args: unknown[]) {
        frames++;
        return Reflect.apply(original, this, args);
      },
    });
    Object.defineProperty(window, "frameDraws", { get: () => frames });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph");
  await canvas.click({ position: { x: 380, y: 160 } });
  const chevron = () =>
    page.evaluate(() =>
      Array.from(
        document.querySelector<HTMLCanvasElement>("#graph")!.getContext("2d")!.getImageData(304, 154, 16, 16).data,
      ),
    );
  const expanded = await chevron(),
    before = await page.evaluate(() => (window as typeof window & { frameDraws: number }).frameDraws);
  await canvas.click({ position: { x: 312, y: 162 } });
  expect(
    (await page.evaluate(() => window.fxnodeExample.root!.getState())).nodes.find((node) => node.id === "math")
      ?.collapsed,
  ).toBe(true);
  await page.waitForTimeout(180);
  const collapsed = await chevron(),
    animated = await page.evaluate(() => (window as typeof window & { frameDraws: number }).frameDraws);
  expect(collapsed).not.toEqual(expanded);
  expect(animated - before).toBeGreaterThan(1);
  await canvas.press("h");
  await page.waitForTimeout(180);
  expect(
    (await page.evaluate(() => window.fxnodeExample.root!.getState())).nodes.find((node) => node.id === "math")
      ?.collapsed,
  ).toBe(false);
  expect(await chevron()).toEqual(expanded);
  await page.evaluate(async () => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      id = (await api.getState()).nodes.find((node) => node.id === "math")!.id;
    return api.dispatch({ type: "node.collapse", id, value: true });
  });
  await page.waitForTimeout(30);
  await page.evaluate(async () => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      id = (await api.getState()).nodes.find((node) => node.id === "math")!.id;
    return api.dispatch({ type: "node.collapse", id, value: false });
  });
  await page.waitForTimeout(180);
  expect(
    (await page.evaluate(() => window.fxnodeExample.root!.getState())).nodes.find((node) => node.id === "math")
      ?.collapsed,
  ).toBe(false);
  expect(await chevron()).toEqual(expanded);
  const idle = await page.evaluate(() => (window as typeof window & { frameDraws: number }).frameDraws);
  await page.waitForTimeout(180);
  expect(await page.evaluate(() => (window as typeof window & { frameDraws: number }).frameDraws)).toBe(idle);
});
