import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const readme = readFileSync(join(root, "README.md"), "utf8");
const fixture = readFileSync(join(root, "test/readme-snippets.compile.ts"), "utf8");
const links = [...readme.matchAll(/!?\[[^\]]*\]\(([^)]+)\)/g)].map((match) => match[1]!);
for (const link of links) {
  if (/^(?:https:\/\/|LICENSE$|NOTICE\.md$)/.test(link)) continue;
  assert.fail(`README link is not package-safe: ${link}`);
}
for (const stale of ["docs/reference/generated", "docs/.vitepress", "](docs/", "](examples/", "](src/"]) {
  assert(!readme.includes(stale), `README contains generated, VitePress-root, or unpacked path: ${stale}`);
}
for (const required of [
  "theme: exampleTheme",
  "createFxNodeHeadless({",
  "api.composeSocket(...numberSocket)",
  "api.composeNode(...valueNode)",
  'window.addEventListener("pagehide", cleanup)',
  "if (cleaned) created.destroy()",
  "api = null",
  "host.destroy()",
  "const gradingWheelsRow = {",
  'satisfies FxNodeDefinition["ui"][number]',
  "async function installColorBalance(api: FxNode)",
]) {
  assert(readme.includes(required), `README current-API regression: ${required}`);
  if (
    ["theme: exampleTheme", "const gradingWheelsRow = {", "async function installColorBalance(api: FxNode)"].includes(
      required,
    )
  )
    assert(
      fixture.includes(
        required
          .replace("const gradingWheelsRow", "export const gradingWheelsRow")
          .replace("async function", "export async function"),
      ),
      `compile fixture is not synchronized: ${required}`,
    );
}
assert(!readme.includes("Immutable current graph snapshot"), "getState must not claim runtime immutability");
console.log(`README checks passed (${readme.split("\n").length - 1} lines, ${links.length} links).`);
