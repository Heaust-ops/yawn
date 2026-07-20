import { mkdir } from "node:fs/promises";
import { chromium } from "playwright";

const snapshotUrl = process.env.SNAPSHOT_URL;
const cdpUrl = process.env.CHROMIUM_CDP_URL ?? "http://127.0.0.1:9222";

if (!snapshotUrl) {
  throw new Error("SNAPSHOT_URL must be the HTTPS Amp portal URL for the demo");
}

await mkdir(".amp/in/artifacts", { recursive: true });

const browser = await chromium.connectOverCDP(cdpUrl);
const context = browser.contexts()[0];
const page = context.pages()[0] ?? (await context.newPage());
const errors = [];

page.on("console", (message) => {
  if (message.type() === "error") {
    errors.push(`[console:error] ${message.text()}`);
  }
});
page.on("pageerror", (error) => {
  errors.push(`[pageerror] ${error.stack || error.message}`);
});

try {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto(snapshotUrl, {
    waitUntil: "domcontentloaded",
    timeout: 60_000,
  });
  await page.waitForFunction(() => globalThis.renderer?.ready === true, null, {
    timeout: 60_000,
  });
  await page.waitForTimeout(5_000);

  const proceduralMeshes = await page.evaluate(
    () => globalThis.renderer.meshes().length,
  );
  await page.screenshot({
    path: ".amp/in/artifacts/yawn-orb-setup-procedural.png",
  });

  await page.selectOption("#scene", "/themanor.glb");
  await page.waitForFunction(
    () => document.querySelector("#load-status")?.textContent.trim() === "Loaded",
    null,
    { timeout: 120_000 },
  );
  await page.waitForTimeout(7_000);

  const manorMeshes = await page.evaluate(
    () => globalThis.renderer.meshes().length,
  );
  await page.screenshot({
    path: ".amp/in/artifacts/yawn-orb-setup-themanor.png",
  });

  if (proceduralMeshes !== 120) {
    errors.push(`expected 120 procedural meshes, found ${proceduralMeshes}`);
  }
  if (manorMeshes !== 514) {
    errors.push(`expected 514 manor meshes, found ${manorMeshes}`);
  }
  if (errors.length) {
    throw new Error(`snapshot check failed:\n${errors.join("\n")}`);
  }

  console.log(`Snapshot check passed: procedural=${proceduralMeshes}, manor=${manorMeshes}`);
} finally {
  await page.goto("about:blank");
  await browser.close();
}
