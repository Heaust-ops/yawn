import test from "node:test";
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { extname, join } from "node:path";
test("browser entry does not import the engine runtime", async () => {
  const source = await readFile(new URL("../src/index.ts", import.meta.url), "utf8");
  assert.doesNotMatch(source, /headless|engine\/engine|core\/document/);
  assert.match(source, /browser\/client/);
});

test("examples server explicitly loads the repository Vite config", async () => {
  const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8")) as {
    scripts?: { example?: string; examples?: string };
  };
  assert.equal(packageJson.scripts?.example, "npm run examples");
  assert.match(packageJson.scripts?.examples ?? "", /vite\s+--config\s+vite\.config\.ts\s+examples(?:\s|$)/);
});

test("standalone examples import library types and values only through the public entrypoint", async () => {
  const root = new URL("../examples/", import.meta.url);
  const directories = ["shared", "minimal", "color-balance", "live-composition"];
  const files: string[] = [];
  async function collect(directory: string): Promise<void> {
    for (const entry of await readdir(new URL(directory, root), { withFileTypes: true })) {
      const relative = join(directory, entry.name);
      if (entry.isDirectory()) await collect(`${relative}/`);
      else if ([".ts", ".tsx", ".js", ".mjs"].includes(extname(entry.name))) files.push(relative);
    }
  }
  await Promise.all(directories.map((directory) => collect(`${directory}/`)));
  for (const file of files) {
    const source = await readFile(new URL(file, root), "utf8");
    const imports = source.matchAll(/(?:from\s*|import\s*)["'](@lib\/[^"']+)["']/g);
    for (const match of imports) assert.equal(match[1], "@lib/index.js", `${file} bypasses the public entrypoint`);
  }
});
