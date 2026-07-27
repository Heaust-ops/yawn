import { expect, test, type Page } from "@playwright/test";

const MAP = {
  knifeX: 600,
  parallelTop: 90,
  parallelBottom: 420,
  chainKnifeX: 430,
  chainTop: 390,
  chainBottom: 485,
  mathAHeader: { x: 850, y: 90 },
  transformHeader: { x: 440, y: 560 },
  noiseHeader: { x: 850, y: 560 },
} as const;

async function open(page: Page) {
  await page.goto("/examples/blender/link-tools-test/");
  await page.evaluate(() => window.linkToolsTest.ready);
  await page.evaluate(() => {
    const events = { mutations: [] as number[], snapshots: [] as number[] };
    Object.assign(window, { linkToolEvents: events });
    window.linkToolsTest.root!.onMutations((e) => events.mutations.push(e.version));
    window.linkToolsTest.root!.onSnapshots((e) => events.snapshots.push(e.version));
  });
  return page.locator("#link-tools");
}

async function publicState(page: Page) {
  return page.evaluate(async () => ({
    save: await window.linkToolsTest.root!.save(),
    snapshot: await window.linkToolsTest.root!.getState(),
    events: structuredClone(
      (window as typeof window & { linkToolEvents: { mutations: number[]; snapshots: number[] } }).linkToolEvents,
    ),
  }));
}

async function knife(
  page: Page,
  canvas: ReturnType<Page["locator"]>,
  from: { x: number; y: number },
  to: { x: number; y: number },
  alt = false,
  release = true,
) {
  await canvas.hover({ position: from });
  await page.keyboard.down("Control");
  if (alt) await page.keyboard.down("Alt");
  await page.mouse.down({ button: "right" });
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas bounds missing");
  await page.mouse.move(box.x + to.x, box.y + to.y, { steps: 12 });
  if (release) await page.mouse.up({ button: "right" });
  if (alt) await page.keyboard.up("Alt");
  await page.keyboard.up("Control");
}

test("Ctrl-RMB knife is transient then removes every crossed link atomically", async ({ page }) => {
  const canvas = await open(page),
    before = await publicState(page);
  await knife(
    page,
    canvas,
    { x: MAP.knifeX, y: MAP.parallelTop },
    { x: MAP.knifeX, y: MAP.parallelBottom },
    false,
    false,
  );
  const during = await publicState(page);
  expect(during).toEqual(before);
  // The actual canvas must contain the white knife/highlight, not merely worker state.
  expect(
    await page.evaluate(
      ({ x, top, bottom }) => {
        const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
          d = c.getContext("2d")!.getImageData(x - 3, top, 7, bottom - top).data;
        let n = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i]! > 220 && d[i + 1]! > 220 && d[i + 2]! > 220) n++;
        return n;
      },
      { x: MAP.knifeX, top: MAP.parallelTop, bottom: MAP.parallelBottom },
    ),
  ).toBeGreaterThan(150);
  await page.mouse.up({ button: "right" });
  await page.keyboard.up("Control");
  const removed = await publicState(page);
  expect(removed.save.links.map((l) => l.id).sort()).toEqual(["chain-in", "chain-out"]);
  expect(removed.snapshot.version).toBe(before.snapshot.version + 1);
  expect(removed.events).toEqual({
    mutations: [before.snapshot.version + 1],
    snapshots: [before.snapshot.version + 1],
  });
  await page.keyboard.press("Control+z");
  const undone = await publicState(page);
  expect(undone.save.links).toEqual(before.save.links);
  expect(undone.snapshot.version).toBe(before.snapshot.version + 2);
});

test("Escape and pointer cancellation discard knife and context menu stays suppressed", async ({ page }) => {
  const canvas = await open(page);
  await page.evaluate(() => {
    (window as any).unblockedMenus = 0;
    document.addEventListener("contextmenu", (event) => {
      if (!event.defaultPrevented) (window as any).unblockedMenus++;
    });
  });
  await knife(page, canvas, { x: 600, y: 55 }, { x: 600, y: 390 }, false, false);
  await page.keyboard.press("Escape");
  await page.mouse.up({ button: "right" });
  await page.keyboard.up("Control");
  expect((await publicState(page)).events).toEqual({ mutations: [], snapshots: [] });
  await knife(page, canvas, { x: 600, y: 55 }, { x: 600, y: 390 }, false, false);
  await canvas.dispatchEvent("pointercancel", {
    pointerId: 1,
    pointerType: "mouse",
    button: 2,
    buttons: 0,
    clientX: 600,
    clientY: 390,
  });
  await page.mouse.up({ button: "right" });
  await page.keyboard.up("Control");
  expect((await publicState(page)).save.links).toHaveLength(5);
  expect(await page.evaluate(() => (window as any).unblockedMenus)).toBe(0);
});

test("Ctrl-Alt-RMB mutes a link, restores its default editor, and undo is atomic", async ({ page }) => {
  const canvas = await open(page),
    before = await publicState(page);
  await knife(page, canvas, { x: 600, y: 100 }, { x: 600, y: 205 }, true);
  const muted = await publicState(page),
    link = muted.save.links.find((l) => l.id === "parallel-a");
  expect(link?.muted).toBe(true);
  expect(muted.events).toEqual({ mutations: [before.snapshot.version + 1], snapshots: [before.snapshot.version + 1] });
  // Red pixels on the formerly grey link and a clickable default-value row prove rendering/hit behavior.
  expect(
    await page.evaluate(() => {
      const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
        d = c.getContext("2d")!.getImageData(500, 50, 200, 130).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i]! > 150 && d[i + 1]! < 110) n++;
      return n;
    }),
  ).toBeGreaterThan(5);
  await canvas.click({ position: { x: 850, y: 125 } });
  expect((await publicState(page)).snapshot.version).toBe(before.snapshot.version + 1);
  await knife(page, canvas, { x: 600, y: 100 }, { x: 600, y: 205 }, true);
  expect((await publicState(page)).save.links.find((l) => l.id === "parallel-a")?.muted).toBe(false);
  await page.keyboard.press("Control+z");
  expect((await publicState(page)).save.links.find((l) => l.id === "parallel-a")?.muted).toBe(true);
});

test("reroute effective mute is red without changing downstream authored flags", async ({ page }) => {
  const canvas = await open(page);
  await knife(page, canvas, { x: MAP.chainKnifeX, y: MAP.chainTop }, { x: MAP.chainKnifeX, y: MAP.chainBottom }, true);
  const state = await publicState(page);
  expect(state.save.links.find((l) => l.id === "chain-in")?.muted).toBe(true);
  expect(state.save.links.find((l) => l.id === "chain-out")?.muted).toBe(false);
  await page.evaluate(() => window.linkToolsTest.view!.whenRendered());
  expect(
    await page.evaluate(() => {
      const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
        d = c.getContext("2d")!.getImageData(590, 420, 250, 100).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i]! > 150 && d[i + 1]! < 110) n++;
      return n;
    }),
  ).toBeGreaterThan(5);
});

test("M mutes operators with bypasses and visibly grays generators", async ({ page }) => {
  const canvas = await open(page);
  for (const [id, point] of [
    ["math-a", MAP.mathAHeader],
    ["transform", MAP.transformHeader],
  ] as const) {
    await canvas.click({ position: point });
    const prior = await publicState(page),
      before = prior.snapshot.version;
    await canvas.press("m");
    const muted = await publicState(page);
    expect(muted.save.nodes.find((n) => n.id === id)?.muted).toBe(true);
    expect(muted.snapshot.version).toBe(before + 1);
    expect(muted.events.mutations).toHaveLength(prior.events.mutations.length + 1);
    expect(muted.events.snapshots).toHaveLength(prior.events.snapshots.length + 1);
    await page.evaluate(() => window.linkToolsTest.view!.whenRendered());
    expect(
      await page.evaluate(({ x, y }) => {
        const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
          d = c.getContext("2d")!.getImageData(x - 100, y, 200, 100).data;
        let n = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i]! > 150 && d[i + 1]! < 110) n++;
        return n;
      }, point),
    ).toBeGreaterThan(5);
    await page.keyboard.press("Control+z");
    expect((await publicState(page)).save.nodes.find((n) => n.id === id)?.muted).toBe(false);
  }
  await canvas.click({ position: MAP.noiseHeader });
  const before = await publicState(page),
    beforeLight = await page.evaluate(() => {
      const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
        d = c.getContext("2d")!.getImageData(780, 550, 165, 85).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) n += d[i]! + d[i + 1]! + d[i + 2]!;
      return n;
    });
  await canvas.press("m");
  await page.evaluate(() => window.linkToolsTest.view!.whenRendered());
  const muted = await publicState(page),
    afterLight = await page.evaluate(() => {
      const c = document.querySelector<HTMLCanvasElement>("#link-tools")!,
        d = c.getContext("2d")!.getImageData(780, 550, 165, 85).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) n += d[i]! + d[i + 1]! + d[i + 2]!;
      return n;
    });
  expect(muted.save.nodes.find((n) => n.id === "noise")?.muted).toBe(true);
  expect(muted.snapshot.version).toBe(before.snapshot.version + 1);
  expect(afterLight).toBeLessThan(beforeLight * 0.8);
  await page.keyboard.press("Control+z");
  expect((await publicState(page)).save.nodes.find((n) => n.id === "noise")?.muted).toBe(false);
});
