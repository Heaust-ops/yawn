import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";

test("browser client has no DOM lifecycle ownership", () => {
  const path = new URL("../src/browser/client.ts", import.meta.url),
    text = readFileSync(path, "utf8"),
    source = ts.createSourceFile(path.pathname, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const forbidden = new Set([
      "document",
      "window",
      "ResizeObserver",
      "MutationObserver",
      "isConnected",
      "addEventListener",
      "removeEventListener",
      "createElement",
      "getBoundingClientRect",
      "focus",
      "setPointerCapture",
      "releasePointerCapture",
      "hasPointerCapture",
      "tabIndex",
      "touchAction",
      "clientWidth",
      "clientHeight",
    ]),
    found = new Set<string>();
  const visit = (node: ts.Node): void => {
    if (ts.isIdentifier(node) && forbidden.has(node.text)) found.add(node.text);
    ts.forEachChild(node, visit);
  };
  visit(source);
  assert.deepEqual([...found], []);
  assert(!text.includes('from "./add-node-menu.js"'));
  assert(!/\b(?:this\.)?canvas\.(?:width|height)\s*=/.test(text));
  assert(!existsSync(new URL("../src/browser/add-node-menu.ts", import.meta.url)));
  const publicIndex = readFileSync(new URL("../src/index.ts", import.meta.url), "utf8");
  assert(publicIndex.includes("FxNodeResourceOpenRequest"));
  assert(!publicIndex.includes("FxNodeResourceOpenTarget"));
  assert(!publicIndex.includes("FxNodeResourceHitRegion"));
});
