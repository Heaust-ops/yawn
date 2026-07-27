import { expect, test } from "@playwright/test";

test("@visual deterministic example canvas", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  await page.evaluate(() => window.fxnodeExample.rendered);

  const canvas = page.locator("#graph");
  await expect(canvas).toHaveAttribute("width", "1200");
  await expect(canvas).toHaveAttribute("height", "640");
  const evidence = await canvas.evaluate((element) => {
    const context = (element as HTMLCanvasElement).getContext("2d");
    if (!context) throw new Error("Canvas context missing");
    const data = context.getImageData(0, 0, 1200, 640).data;
    let background = 0,
      nodePixels = 0,
      linkPixels = 0;
    for (let index = 0; index < data.length; index += 4) {
      const r = data[index],
        g = data[index + 1],
        b = data[index + 2],
        a = data[index + 3];
      if (r === 29 && g === 31 && b === 35 && a === 255) background++;
      if (r === 53 && g === 56 && b === 62 && a === 255) nodePixels++;
      if ((r === 168 && g === 168 && b === 168) || (r === 98 && g === 179 && b === 79)) linkPixels++;
    }
    return { background, nodePixels, linkPixels };
  });
  expect(evidence.background).toBeGreaterThan(300_000);
  expect(evidence.nodePixels).toBeGreaterThan(20_000);
  expect(evidence.linkPixels).toBeGreaterThan(100);

  const first = await canvas.screenshot();
  const second = await canvas.screenshot();
  expect(second.equals(first)).toBe(true);
  await expect(canvas).toHaveScreenshot("phase-4-example.png", { animations: "disabled" });

  await canvas.click({ position: { x: 380, y: 160 } });
  await canvas.click({ position: { x: 560, y: 140 }, modifiers: ["Shift"] });
  await page.evaluate(() => window.fxnodeExample.view!.whenRendered());
  const selection = await canvas.evaluate((element) => {
    const context = (element as HTMLCanvasElement).getContext("2d")!;
    const pixels = context.getImageData(0, 0, 1200, 640).data;
    let selected = 0,
      expanded = 0;
    for (let y = 150; y < 260; y++)
      for (let x = 295; x < 485; x++) {
        const index = (y * 1200 + x) * 4;
        if (pixels[index] === 237 && pixels[index + 1] === 87 && pixels[index + 2] === 0 && pixels[index + 3] === 255) {
          selected++;
          if (x < 299) expanded++;
        }
      }
    const socketIndex = (234 * 1200 + 300) * 4;
    return { selected, expanded, socket: Array.from(pixels.slice(socketIndex, socketIndex + 4)) };
  });
  expect(selection.selected).toBeGreaterThan(100);
  expect(selection.expanded).toBe(0);
  expect(selection.socket).toEqual([168, 168, 168, 255]);
});

test("@visual distant zoom keeps text inside scaled nodes", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph");
  await canvas.hover({ position: { x: 600, y: 320 } });
  await page.mouse.wheel(0, 900);
  await page.evaluate(() => window.fxnodeExample.view!.whenRendered());
  await expect(canvas).toHaveScreenshot("zoomed-out-lod.png", { animations: "disabled" });
});

test("@visual Blender-style numeric fields and text editing", async ({ page }) => {
  await page.goto("/examples/blender/control-test/");
  await page.evaluate(() => window.controlTest.ready);
  const canvas = page.locator("#controls");
  await expect(canvas).toHaveScreenshot("numeric-fields.png", { animations: "disabled" });
  await canvas.click({ position: { x: 196, y: 147 } });
  await expect(canvas).toHaveScreenshot("numeric-editing.png", { animations: "disabled" });
  await canvas.press("Escape");
  await canvas.click({ position: { x: 1003, y: 147 } });
  await expect(canvas).toHaveScreenshot("color-picker.png", { animations: "disabled" });
});
