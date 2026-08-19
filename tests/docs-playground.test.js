import test from "node:test";
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

import {
  PLAYGROUND_RECIPES,
  playgroundRecipe,
} from "../examples/playground/recipes.js";

const root = path.resolve(import.meta.dirname, "..");
const docsRoot = path.join(root, "docs");

async function markdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map((entry) => {
      const file = path.join(directory, entry.name);
      return entry.isDirectory()
        ? markdownFiles(file)
        : entry.name.endsWith(".md")
          ? [file]
          : [];
    }),
  );
  return nested.flat();
}

test("playground recipes are unique, compilable, and linked to docs", async () => {
  const entries = Object.entries(PLAYGROUND_RECIPES);
  assert.equal(entries.length, 7);
  assert.equal(playgroundRecipe("unknown"), PLAYGROUND_RECIPES["first-scene"]);

  const sources = new Set();
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  for (const [id, recipe] of entries) {
    assert.match(id, /^[a-z][a-z0-9-]+$/);
    assert.ok(!sources.has(recipe.source), `${id} must have unique editable code`);
    sources.add(recipe.source);
    assert.doesNotThrow(() => new AsyncFunction("yawn", recipe.source));

    const relativePage = recipe.docs
      .replace(/^\/docs\//, "")
      .split("#", 1)[0]
      .replace(/\/$/, "/index");
    const markdown = await readFile(
      path.join(docsRoot, `${relativePage}.md`),
      "utf8",
    );
    assert.ok(markdown.length > 0, `${id} docs page must exist`);
  }
});

test("docs contain every former recipe and only link to playground examples", async () => {
  const files = await markdownFiles(docsRoot);
  const contents = await Promise.all(files.map((file) => readFile(file, "utf8")));
  const documentation = contents.join("\n");

  for (let number = 1; number <= 17; number++) {
    const prefix = String(number).padStart(2, "0");
    assert.match(documentation, new RegExp(`^## ${prefix} —`, "m"));
  }
  for (const id of Object.keys(PLAYGROUND_RECIPES)) {
    assert.match(documentation, new RegExp(`id=["']${id}["']`));
  }

  const examplesIndex = await readFile(path.join(root, "examples/index.html"), "utf8");
  assert.doesNotMatch(examplesIndex, /cookbook|\.\/[^\s"']+\.js/i);
  assert.match(examplesIndex, /\/playground\//);
});
