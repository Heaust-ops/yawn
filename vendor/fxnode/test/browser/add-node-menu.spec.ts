import { expect, test } from "@playwright/test";

test("right-click DOM menu searches and adds through one worker gesture", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  await page.evaluate(() => {
    const events: {
      mutations: Array<{ version: number; cause: string; mutations: readonly unknown[] }>;
      snapshots: number[];
      requests: Array<{ frozen: boolean; revision: number; position: { x: number; y: number } }>;
    } = { mutations: [], snapshots: [], requests: [] };
    (window as typeof window & { menuEvents: typeof events }).menuEvents = events;
    window.root.onMutations((event) => events.mutations.push(event));
    window.root.onSnapshots((event) => events.snapshots.push(event.version));
    window.view.onHostRequests((request) => {
      if (request.kind === "add-node-menu")
        events.requests.push({
          frozen: Object.isFrozen(request) && Object.isFrozen(request.viewPosition),
          revision: request.compositionRevision,
          position: { ...request.viewPosition },
        });
    });
  });
  const canvas = page.locator("#primary");
  await canvas.click({ button: "right", position: { x: 40, y: 50 } });
  const dialog = page.getByRole("dialog", { name: "Add node" });
  await expect(dialog).toBeVisible();
  expect(
    await dialog.getByRole("group").evaluateAll((groups) =>
      groups.map((group) => ({
        name: group.getAttribute("aria-label"),
        options: [...group.querySelectorAll('[role="option"]')].map((item) => item.textContent?.trim()),
      })),
    ),
  ).toEqual([
    { name: "Common", options: ["Frame", "Reroute", "Group Input", "Group Output"] },
    {
      name: "Shader",
      options: [
        "Value",
        "Color",
        "Math",
        "Vector Math",
        "Mix",
        "Color Ramp",
        "Texture Coordinate",
        "Noise Texture",
        "Image Texture",
        "Principled BSDF",
        "Material Output",
      ],
    },
    { name: "Geometry", options: ["Position", "Mesh Cube", "Set Position", "Transform Geometry", "Join Geometry"] },
    { name: "Compositor", options: ["Image", "Color Balance"] },
  ]);
  const search = page.getByRole("combobox", { name: "Search nodes" });
  await search.fill("  ShAdEr  ");
  await expect(dialog.getByRole("option")).toHaveText([
    "Value",
    "Color",
    "Math",
    "Vector Math",
    "Mix",
    "Color Ramp",
    "Texture Coordinate",
    "Noise Texture",
    "Image Texture",
    "Principled BSDF",
    "Material Output",
  ]);
  await search.fill("noise-texture");
  await expect(dialog.getByRole("option")).toHaveText(["Noise Texture"]);
  await search.press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => window.root.getState().then((snapshot) => snapshot.nodes.length)))
    .toBe(1);
  const result = await page.evaluate(async () => ({
    snapshot: await window.root.getState(),
    events: (
      window as typeof window & {
        menuEvents: {
          mutations: Array<{ version: number; cause: string; mutations: readonly unknown[] }>;
          snapshots: number[];
          requests: Array<{ frozen: boolean; revision: number; position: { x: number; y: number } }>;
        };
      }
    ).menuEvents,
  }));
  expect(result.snapshot.nodes[0]?.typeId).toBe("fxnode.shader.noise-texture");
  expect(result.snapshot.nodes[0]?.position).toEqual({ x: -120, y: 40 });
  expect(result.events.mutations).toHaveLength(1);
  expect(result.events.mutations[0]).toMatchObject({ version: 2, cause: "api" });
  expect(result.events.mutations[0]?.mutations).toHaveLength(1);
  expect(result.events.snapshots).toEqual([2]);
  expect(result.events.requests).toEqual([{ frozen: true, revision: 31, position: { x: 40, y: 50 } }]);
  const rootAdded = await page.evaluate(async () => {
    const beforeSelection = window.view.getHostSnapshot().selection.nodeCount;
    await window.root.dispatch({ type: "node.add", nodeType: "fxnode.shader.value", position: { x: -120, y: 40 } });
    const snapshot = await window.root.getState();
    return {
      id: snapshot.nodes.find((node) => node.typeId === "fxnode.shader.value")!.id,
      beforeSelection,
      afterSelection: window.view.getHostSnapshot().selection.nodeCount,
    };
  });
  expect(rootAdded.afterSelection).toBe(rootAdded.beforeSelection);
  await canvas.press("Delete");
  expect((await page.evaluate(() => window.root.getState())).nodes.map((node) => node.id)).toEqual([rootAdded.id]);
  await windowUndo(page);
  await windowUndo(page);
  await windowUndo(page);
  expect((await page.evaluate(() => window.root.getState())).nodes).toHaveLength(0);
  await canvas.click({ button: "right", position: { x: 80, y: 80 } });
  await expect(dialog).toBeVisible();
  await page.evaluate(async () => {
    window.fxnodeHost.destroy();
    await window.view.detach();
    window.root.destroy();
  });
  await expect(dialog).toHaveCount(0);
});

test("right-click on a node and Ctrl-RMB never open the add menu", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph"),
    dialog = page.getByRole("dialog", { name: "Add node" });
  await canvas.click({ button: "right", position: { x: 320, y: 160 } });
  await expect(dialog).toHaveCount(0);
  await canvas.hover({ position: { x: 30, y: 30 } });
  await page.keyboard.down("Control");
  await page.mouse.down({ button: "right" });
  await page.mouse.up({ button: "right" });
  await page.keyboard.up("Control");
  await expect(dialog).toHaveCount(0);
});

async function windowUndo(page: import("@playwright/test").Page): Promise<void> {
  await page.evaluate(() => window.root.undo());
}
