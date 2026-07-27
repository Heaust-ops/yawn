import { expect, test } from "@playwright/test";
for (const example of ["minimal", "color-balance", "live-composition", "multi-view"])
  test(`${example} documentation image`, async ({ page }) => {
    await page.goto(`/examples/${example}/`);
    await page.evaluate(
      (name) => (name === "multi-view" ? window.fxnodeMultiView.ready : window.fxnodeStandalone.ready),
      example,
    );
    await expect(page).toHaveScreenshot(`${example}.png`, { animations: "disabled", fullPage: true });
  });
