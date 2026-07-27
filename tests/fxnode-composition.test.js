import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "vite";
import { fxNodeComposition } from "../static/render-graph/catalog.js";

test("production render graph composition passes fxnode's public validator", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "yawn-fxnode-validator-"));
  try {
    const entry = path.join(directory, "entry.js");
    await writeFile(entry, `export { validateFxNodeComposition } from ${JSON.stringify(pathToFileURL(path.resolve("vendor/fxnode/src/index.ts")).href)};`);
    await build({
      configFile: false,
      logLevel: "silent",
      build: { lib: { entry, formats: ["es"], fileName: "validator" }, outDir: directory, emptyOutDir: false },
    });
    const { validateFxNodeComposition } = await import(`${pathToFileURL(path.join(directory, "validator.js")).href}?${Date.now()}`);
    const result = validateFxNodeComposition(fxNodeComposition);
    assert.equal(result.ok, true, result.ok ? undefined : JSON.stringify(result.issues, null, 2));
    assert.equal(Object.keys(fxNodeComposition.nodes).length, 17);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
