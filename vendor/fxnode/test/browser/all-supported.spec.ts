import { expect, test } from "@playwright/test";
test("@visual all supported catalog", async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto("/examples/blender/all-supported/");
  await page.waitForFunction(() => Boolean(window.fxnodeExample));
  await page.evaluate(() => window.fxnodeExample.ready);
  await page.locator("canvas").press("Home");
  await page.evaluate(() => window.fxnodeExample.view!.whenRendered());
  await expect(page.locator("canvas")).toHaveScreenshot("all-supported.png", { animations: "disabled" });
});
