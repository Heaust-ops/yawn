import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "vite";
import { fxNodeComposition } from "../static/render-graph/catalog.js";

test("production render graph composition passes fxnode's public validator", async () => {
  const directory = await mkdtemp(
    path.join(tmpdir(), "yawn-fxnode-validator-"),
  );
  try {
    const entry = path.join(directory, "entry.js");
    await writeFile(
      entry,
      `export { validateFxNodeComposition } from ${JSON.stringify(pathToFileURL(path.resolve("vendor/fxnode/src/index.ts")).href)};`,
    );
    await build({
      configFile: false,
      logLevel: "silent",
      build: {
        lib: { entry, formats: ["es"], fileName: "validator" },
        outDir: directory,
        emptyOutDir: false,
      },
    });
    const { validateFxNodeComposition } = await import(
      `${pathToFileURL(path.join(directory, "validator.js")).href}?${Date.now()}`
    );
    const result = validateFxNodeComposition(fxNodeComposition);
    assert.equal(
      result.ok,
      true,
      result.ok ? undefined : JSON.stringify(result.issues, null, 2),
    );
    assert.equal(fxNodeComposition.schemaVersion, 2);
    assert.equal(fxNodeComposition.version, 8);
    assert.equal(Object.keys(fxNodeComposition.nodes).length, 42);
    assert.ok(
      Object.values(fxNodeComposition.nodes).every(
        (definition) => definition.migrations.length === 0,
      ),
    );
    for (const [type, descriptor] of Object.entries(fxNodeComposition.nodes)) {
      const socketKeys = Object.keys(descriptor.sockets);
      assert.equal(
        socketKeys.length,
        new Set(socketKeys).size,
        `${type} has colliding input and output socket names`,
      );
    }
    assert.deepEqual(fxNodeComposition.nodes.not.sockets.operand, {
      title: "operand",
      direction: "input",
      type: "bool",
      maxIncomingLinks: 1,
      visible: true,
      value: { type: "boolean", default: { kind: "boolean", value: false } },
      showValue: true,
    });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
