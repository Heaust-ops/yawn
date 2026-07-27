import { expect, test } from "@playwright/test";

test("renderer paints composition-resolved background, header, socket, and link colors", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/test/browser/presentation-render.html");
  await page.waitForTimeout(500);
  expect(errors).toEqual([]);
  const result = await page.evaluate(
    () =>
      (
        window as unknown as {
          presentationResult: {
            background: number[];
            header: number[];
            socket: number[];
            linkContainsCustomColor: boolean;
          };
        }
      ).presentationResult,
  );
  expect(result).toEqual({
    background: [1, 2, 3, 255],
    header: [118, 84, 171, 255],
    socket: [161, 178, 195, 255],
    linkContainsCustomColor: true,
  });
});
