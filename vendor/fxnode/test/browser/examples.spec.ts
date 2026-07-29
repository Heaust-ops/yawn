import { expect, test, type Page } from "@playwright/test";

const examples = [
  { path: "minimal", nodeId: "value", typeId: "example.minimal.value" },
  { path: "color-balance", nodeId: "color-balance", typeId: "fxnode.compositor.color-balance" },
  { path: "live-composition", nodeId: "live-node", typeId: "example.live.parameter" },
  { path: "logic-nodes", nodeId: "and", typeId: "example.logic.and" },
] as const;

function capturePageErrors(page: Page): Error[] {
  const errors: Error[] = [];
  page.on("pageerror", (error) => errors.push(error));
  return errors;
}

test("gallery links every standalone application and loads its images", async ({ page }) => {
  await page.goto("/examples/");
  await expect(page.locator(".gallery a")).toHaveCount(6);
  expect(
    await page.locator(".gallery a").evaluateAll((links) => links.map((link) => link.getAttribute("href"))),
  ).toEqual(["./minimal/", "./color-balance/", "./live-composition/", "./logic-nodes/", "./multi-view/", "./blender/"]);
  await expect(page.locator(".gallery img")).toHaveCount(5);
  expect(
    await page
      .locator(".gallery img")
      .evaluateAll((images) =>
        images.every((image) => image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0),
      ),
  ).toBe(true);
});

test("multi-view keeps view input and selection local while graph changes fan out", async ({ page }) => {
  await page.addInitScript(() => {
    const original = EventTarget.prototype.addEventListener;
    (window as unknown as { canvasListeners: Record<string, string[]> }).canvasListeners = {};
    EventTarget.prototype.addEventListener = function (type, listener, options) {
      if (this instanceof HTMLCanvasElement) {
        const events = (window as unknown as { canvasListeners: Record<string, string[]> }).canvasListeners;
        (events[this.id] ??= []).push(type);
      }
      return original.call(this, type, listener, options);
    };
  });
  await page.goto("/examples/multi-view/");
  await page.evaluate(() => window.fxnodeMultiView.ready);
  expect(
    await page.evaluate(() => (window as unknown as { canvasListeners: Record<string, string[]> }).canvasListeners),
  ).toEqual({
    "view-a": [
      "pointerdown",
      "pointerdown",
      "pointermove",
      "pointerup",
      "pointercancel",
      "mousedown",
      "wheel",
      "keydown",
      "keyup",
      "focus",
      "blur",
      "contextmenu",
      "lostpointercapture",
    ],
    "view-b": [
      "pointerdown",
      "pointerdown",
      "pointermove",
      "pointerup",
      "pointercancel",
      "mousedown",
      "wheel",
      "keydown",
      "keyup",
      "focus",
      "blur",
      "contextmenu",
      "lostpointercapture",
    ],
  });

  const baseline = await page.evaluate(async () => ({
    renders: [...window.fxnodeMultiView.renderCounts],
    ids: (await window.fxnodeMultiView.root!.getState()).nodes.map((node) => node.id),
  }));
  await page.getByRole("button", { name: "Add" }).click();
  await expect
    .poll(() => page.evaluate(() => window.fxnodeMultiView.root!.getState().then((state) => state.nodes.length)))
    .toBe(baseline.ids.length + 1);
  await page.locator("#view-b").dispatchEvent("pointerdown", {
    pointerId: 2,
    pointerType: "mouse",
    clientX: 900,
    clientY: 300,
    button: 0,
    buttons: 1,
  });
  await expect(page.locator("article").nth(1)).toHaveClass(/active/);
  await page.getByRole("button", { name: "Add" }).click();
  await expect
    .poll(() => page.evaluate(() => window.fxnodeMultiView.root!.getState().then((state) => state.nodes.length)))
    .toBe(baseline.ids.length + 2);
  await expect
    .poll(() =>
      page.evaluate(
        (counts) => window.fxnodeMultiView.renderCounts.every((value, i) => value > counts[i]!),
        baseline.renders,
      ),
    )
    .toBe(true);

  let result = await page.evaluate(async (baselineIds) => {
    const state = await window.fxnodeMultiView.root!.getState();
    const added = state.nodes.filter((node) => !baselineIds.includes(node.id));
    return {
      positions: added.map((node) => node.position),
      selections: window.fxnodeMultiView.views.map((view) => view.getHostSnapshot().selection.nodeCount),
    };
  }, baseline.ids);
  expect(result.positions.sort((a, b) => a.x - b.x)).toEqual([
    { x: 480, y: -550 },
    { x: 2080, y: -400 },
  ]);
  expect(result.selections).toEqual([1, 1]);

  await page.getByRole("button", { name: "Mute" }).click();
  await expect(page.getByRole("button", { name: "Unmute" })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Delete" }).click();
  await expect
    .poll(() => page.evaluate(() => window.fxnodeMultiView.root!.getState().then((state) => state.nodes.length)))
    .toBe(baseline.ids.length + 1);
  result = await page.evaluate(
    async (baselineIds) => ({
      positions: (await window.fxnodeMultiView.root!.getState()).nodes
        .filter((node) => !baselineIds.includes(node.id))
        .map((node) => node.position),
      selections: window.fxnodeMultiView.views.map((view) => view.getHostSnapshot().selection.nodeCount),
    }),
    baseline.ids,
  );
  expect(result.positions).toEqual([{ x: 480, y: -550 }]);
  expect(result.selections).toEqual([1, 0]);
  await page.evaluate(() => Promise.all([window.fxnodeMultiView.cleanup(), window.fxnodeMultiView.cleanup()]));
  expect(
    await page.evaluate(() => ({ root: window.fxnodeMultiView.root, views: window.fxnodeMultiView.views.length })),
  ).toEqual({ root: null, views: 0 });
});

test("logic nodes accept five links through one input and application evaluation follows snapshots", async ({
  page,
}) => {
  const errors = capturePageErrors(page);
  await page.goto("/examples/logic-nodes/");
  await page.evaluate(() => window.fxnodeStandalone.ready);
  const initial = await page.evaluate(async () => {
    const state = await window.fxnodeStandalone.root!.getState();
    return {
      incoming: state.links.filter((link) => link.toSocketId === "and:inputs").length,
      labels: [...document.querySelectorAll<HTMLElement>("#results span")].map((item) => item.textContent),
    };
  });
  expect(initial.incoming).toBe(5);
  expect(initial.labels).toContain("AND: false");

  await page.evaluate(async () => {
    const root = window.fxnodeStandalone.root!;
    const source = (await root.getState()).nodes.find((node) => node.id === "c")!;
    return root.dispatch({
      type: "node.parameter",
      id: source.id,
      key: "value",
      value: { kind: "boolean", value: true },
    });
  });
  await expect(page.locator("#results span").filter({ hasText: "AND:" })).toHaveText("AND: true");
  expect(errors).toEqual([]);
});

for (const example of examples) {
  test(`${example.path} renders its known node and cleans up on pagehide`, async ({ page }) => {
    const errors = capturePageErrors(page);
    await page.goto(`/examples/${example.path}/`);
    await page.evaluate(() => window.fxnodeStandalone.ready);
    const result = await page.evaluate(async ({ nodeId }) => {
      const api = window.fxnodeStandalone.root;
      if (!api) return null;
      const node = (await api.getState()).nodes.find((candidate) => candidate.id === nodeId);
      const canvas = document.querySelector<HTMLCanvasElement>("canvas")!;
      const pixels = canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height).data;
      return {
        node: node && { id: node.id, typeId: node.typeId, known: node.known },
        nonEmpty: pixels.some((channel) => channel !== 0),
      };
    }, example);
    expect(result).not.toBeNull();
    expect(result?.node).toEqual({ id: example.nodeId, typeId: example.typeId, known: true });
    expect(result?.nonEmpty).toBe(true);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide")));
    expect(await page.evaluate(() => window.fxnodeStandalone.root)).toBeNull();
    expect(errors).toEqual([]);
  });
}

test("live composition migrates the real node and records the graph/history transition", async ({ page }) => {
  const errors = capturePageErrors(page);
  await page.goto("/examples/live-composition/");
  await page.evaluate(() => window.fxnodeStandalone.ready);
  const beforeVersion = await page.evaluate(() => window.fxnodeStandalone.graphVersion);
  expect(beforeVersion).toBeDefined();
  await page.getByRole("button", { name: "Compose version 2" }).click();
  await expect(page.locator("#status")).toContainText("Version 2 committed");
  const result = await page.evaluate(async () => ({
    receipt: window.fxnodeStandalone.lastCompositionReceipt,
    state: await window.fxnodeStandalone.root!.getState(),
  }));
  expect(result.receipt?.status).toBe("committed");
  expect(result.receipt?.graphChanged).toBe(true);
  expect(result.receipt?.historyReset).toBe(true);
  expect(result.receipt?.graphVersion).toBe(beforeVersion! + 1);
  const node = result.state.nodes.find((candidate) => candidate.id === "live-node");
  expect(node?.typeId).toBe("example.live.parameter");
  expect(node?.typeVersion).toBe(2);
  expect(node?.parameters["detail"]).toEqual({ kind: "number", value: 0.5 });
  expect(errors).toEqual([]);
});
