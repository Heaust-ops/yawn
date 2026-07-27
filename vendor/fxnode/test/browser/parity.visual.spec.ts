import { expect, test, type Locator } from "@playwright/test";

async function stable(canvas: Locator): Promise<void> {
  const first = await canvas.screenshot();
  await canvas.page().evaluate(() => window.parityExample.view.whenRendered());
  expect((await canvas.screenshot()).equals(first)).toBe(true);
}

test("@visual structural parity baseline and focused controls", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto("/examples/blender/parity/");
  await page.waitForFunction(() => Boolean(window.parityExample));
  const canvas = page.locator("canvas");
  await canvas.press("Home");
  await page.evaluate(() => window.parityExample.view.whenRendered());
  await stable(canvas);
  const regions = await canvas.evaluate((element) => {
    const context = (element as HTMLCanvasElement).getContext("2d")!;
    const regions = [
      { x: 40, y: 110, w: 260, h: 290 },
      { x: 1025, y: 110, w: 370, h: 330 },
      { x: 650, y: 615, w: 470, h: 240 },
    ];
    return regions.map((region) => {
      const data = context.getImageData(region.x, region.y, region.w, region.h).data;
      let body = 0,
        controls = 0;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i] === 53 && data[i + 1] === 56 && data[i + 2] === 62) body++;
        if (data[i] === 36 && data[i + 1] === 39 && data[i + 2] === 43) controls++;
      }
      return { body, controls };
    });
  });
  for (const region of regions) expect(region.body).toBeGreaterThan(1000);
  expect(regions.reduce((sum, region) => sum + region.controls, 0)).toBeGreaterThan(500);
  await expect(canvas).toHaveScreenshot("parity-structural.png", { animations: "disabled" });

  // Crops deliberately cover the widget structures rather than Blender pixels:
  // image selectors/projection, grading mode rows, and ramp/noise conditional rows.
  const imageGrading = await page.screenshot({
    clip: { x: 90, y: 500, width: 1260, height: 420 },
    animations: "disabled",
  });
  expect(imageGrading).toMatchSnapshot("parity-image-grading.png");
  const rampNoise = await page.screenshot({
    clip: { x: 380, y: 55, width: 1060, height: 450 },
    animations: "disabled",
  });
  expect(rampNoise).toMatchSnapshot("parity-ramp-noise.png");
});
