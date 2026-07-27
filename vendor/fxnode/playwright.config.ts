import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "test/browser",
  testIgnore: /.*visual\.spec\.ts/,
  grepInvert: /@visual/,
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
  ],
  use: { baseURL: "http://127.0.0.1:4173" },
  webServer: { command: "npx vite --host 127.0.0.1 --port 4173", port: 4173, reuseExistingServer: false },
  timeout: 20_000,
  snapshotPathTemplate: "{testDir}/{testFilePath}-snapshots/{arg}-{platform}{ext}",
});
