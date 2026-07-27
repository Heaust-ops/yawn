import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "test/browser",
  // Keep the compact regression, catalog overview, and focused parity evidence
  // in one Chromium-only visual run.
  testMatch: ["**/visual.spec.ts", "**/all-supported.spec.ts", "**/parity.visual.spec.ts"],
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  use: { baseURL: "http://127.0.0.1:4173", deviceScaleFactor: 1 },
  webServer: { command: "npx vite --host 127.0.0.1 --port 4173", port: 4173, reuseExistingServer: false },
  timeout: 20_000,
  snapshotPathTemplate: "{testDir}/{testFilePath}-snapshots/{arg}-{platform}{ext}",
});
