import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "test/browser",
  testMatch: "**/examples.docs.visual.spec.ts",
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium", viewport: { width: 1200, height: 720 }, deviceScaleFactor: 1 },
    },
  ],
  use: { baseURL: "http://127.0.0.1:4173" },
  webServer: {
    command: "npx vite --config vite.config.ts --host 127.0.0.1 --port 4173",
    port: 4173,
    reuseExistingServer: false,
  },
  snapshotPathTemplate: "examples/assets/{arg}.png",
  timeout: 20_000,
});
