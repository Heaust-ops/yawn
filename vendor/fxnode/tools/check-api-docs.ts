import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const docs = join(root, "docs");
const output = join(root, "docs/reference/generated");
const files = readdirSync(output, { recursive: true }).map(String);
const sidebarFile = files.find((file) => /sidebar.*\.json$/i.test(file));
assert(sidebarFile, "VitePress sidebar JSON was not generated");
const sidebar: unknown = JSON.parse(readFileSync(join(output, sidebarFile), "utf8"));
assert(Array.isArray(sidebar), "sidebar root must be the observed VitePress array structure");

type SidebarItem = { text?: unknown; link?: unknown; items?: unknown };
const sidebarItems: SidebarItem[] = [];
const visit = (items: unknown): void => {
  assert(Array.isArray(items), "sidebar items must be arrays");
  for (const item of items) {
    assert(item && typeof item === "object", "sidebar entries must be objects");
    const typed = item as SidebarItem;
    sidebarItems.push(typed);
    if (typed.items !== undefined) visit(typed.items);
  }
};
visit(sidebar);
for (const item of sidebarItems) {
  if (item.link === undefined) continue;
  assert(typeof item.link === "string", "sidebar links must be strings");
  const link = item.link;
  const relative = link.replace(/^\/reference\/generated\//, "");
  assert(relative !== link, `sidebar link is outside generated reference: ${link}`);
  const target = relative.endsWith("/") ? `${relative}index.md` : relative;
  assert(existsSync(join(output, target)), `broken sidebar link: ${link}`);
}
const packageModules = sidebarItems
  .filter(
    (item): item is SidebarItem & { text: string; link: string } =>
      typeof item.text === "string" && typeof item.link === "string" && item.link.endsWith("/"),
  )
  .map(({ text, link }) => ({ text, link }));
assert.deepEqual(packageModules, [
  { text: "fxnode", link: "/reference/generated/fxnode/" },
  { text: "headless", link: "/reference/generated/fxnode/headless/" },
  { text: "color-ramp", link: "/reference/generated/fxnode/widgets/color-ramp/" },
]);

const text = files
  .filter((file) => file.endsWith(".md") || file.endsWith(".json"))
  .map((file) => readFileSync(join(output, file), "utf8"))
  .join("\n");
for (const moduleName of ["fxnode", "fxnode/headless", "fxnode/widgets/color-ramp"])
  assert(text.includes(moduleName), `missing documented package module: ${moduleName}`);
for (const denied of ["BoundEngine", "BoundDocument", "bindEngine", "bindDocument", "@lib/"])
  assert(!text.includes(denied), `implementation name leaked into docs: ${denied}`);
for (const line of text.split("\n").filter((line) => line.startsWith("Defined in:")))
  assert.match(
    line,
    /^Defined in: \[[^\]]+:\d+\]\(https:\/\/github\.com\/Heaust-ops\/fxnode\/blob\/main\/[A-Za-z0-9._/-]+#L\d+\)$/,
    `invalid source location: ${line}`,
  );
for (const denied of ["/home/", "file:", "node\\_modules/", "node_modules/"])
  assert(!text.includes(denied), `local or dependency path leaked into docs: ${denied}`);

// VitePress treats a relative link outside docs as a page link and can fail to
// report a missing repository source file. Repository TypeScript links must be
// durable GitHub links; relative image links deliberately remain supported.
const markdownFiles = readdirSync(docs, { recursive: true })
  .map(String)
  .filter((file) => file.endsWith(".md") && !file.startsWith("reference/generated/"));
for (const file of markdownFiles) {
  const markdown = readFileSync(join(docs, file), "utf8");
  for (const match of markdown.matchAll(/(?<!!)\[[^\]]*\]\(([^)]+\.ts(?:#[^)]*)?)\)/g)) {
    const target = match[1]!;
    assert(
      /^https?:\/\//.test(target),
      `${file}: repository TypeScript source link must be an absolute URL: ${target}`,
    );
  }
}

console.log(
  `Docs passed (${files.filter((file) => file.endsWith(".md")).length} generated API pages, ${markdownFiles.length} authored pages; ${sidebarFile})`,
);
