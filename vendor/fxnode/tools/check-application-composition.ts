import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "../test/application.js";
import { APPLICATION_VERSION } from "../examples/blender/nodes/application.js";

const ids = [...APPLICATION_COMPILED.nodes.keys()];
if (ids.length !== 22 || new Set(ids).size !== ids.length)
  throw new Error(`Application composition must contain 22 unique nodes; found ${ids.length}`);
if (APPLICATION_COMPILED.nodes.size !== ids.length)
  throw new Error("Compiled application composition coverage differs from source");
if (APPLICATION_COMPILED.version !== APPLICATION_VERSION)
  throw new Error("Compiled application version differs from its declared version");
for (const [index, id] of ids.entries())
  if (APPLICATION_HEADLESS.materializeNode(`check-${index}`, id).typeId !== id)
    throw new Error(`Could not materialize ${id}`);
const root = new URL("..", import.meta.url).pathname,
  src = join(root, "src");
if (existsSync(join(src, "catalog"))) throw new Error("Published source must not contain an application catalog");
const productionFiles = readdirSync(src, { recursive: true })
  .map(String)
  .filter((file) => /\.(?:ts|tsx)$/.test(file));
const productionText = productionFiles.map((file) => readFileSync(join(src, file), "utf8")).join("\n");
for (const forbidden of [
  /fxnode\.(?:common|shader|geometry|compositor)\./,
  /\b(?:LEGACY_COMPOSITION|BUILTIN_DESCRIPTORS|DESCRIPTOR_REGISTRY|CATALOG_NODE_IDS|BLENDER_DARK_THEME|BuiltinNodeTypeId|RenderTheme)\b/,
  /from\s+["'][^"']*examples\//,
])
  if (forbidden.test(productionText)) throw new Error(`Concrete application authority leaked into src: ${forbidden}`);
const exampleRoot = join(root, "examples/blender");
const nodeRoot = join(exampleRoot, "nodes");
const categoryIndexes = existsSync(nodeRoot)
  ? readdirSync(nodeRoot, { recursive: true })
      .map(String)
      .filter((file) => file.endsWith("/index.ts"))
  : [];
if (categoryIndexes.length) throw new Error(`Category helper indexes are forbidden: ${categoryIndexes.join(", ")}`);
if (existsSync(join(exampleRoot, "application-composition.ts")))
  throw new Error("Deleted application aggregate was restored");
if (existsSync(join(exampleRoot, "application-runtime.ts")))
  throw new Error("Application runtime was restored under example");
for (const browserFile of [
  "application-browser.ts",
  "control-test/main.ts",
  "link-tools-test/main.ts",
  "ramp-test/main.ts",
]) {
  const text = readFileSync(join(exampleRoot, browserFile), "utf8");
  if (/test\/application/.test(text)) throw new Error(`${browserFile} imports the test-only application authority`);
}
for (const externalRoot of ["examples", "test", "tools"])
  for (const file of readdirSync(join(root, externalRoot), { recursive: true }).map(String)) {
    if (!/\.tsx?$/.test(file)) continue;
    const text = readFileSync(join(root, externalRoot, file), "utf8");
    if (/(?:from\s*|import\s*\()["'](?:\.\.\/)+src(?:\/|["'])/.test(text))
      throw new Error(`${externalRoot}/${file} must import library source through @lib/`);
  }
for (const sibling of ["minimal", "color-balance", "live-composition"])
  for (const file of readdirSync(join(root, "examples", sibling), { recursive: true }).map(String)) {
    if (!/\.tsx?$/.test(file)) continue;
    const text = readFileSync(join(root, "examples", sibling, file), "utf8");
    if (/blender\/(?:application-browser|main)|test\/application/.test(text))
      throw new Error(`examples/${sibling}/${file} imports Blender or test application bootstrap`);
  }
const authoringFiles = [
  join(root, "test/application.ts"),
  join(root, "examples/shared/nodes/color-balance.ts"),
  ...(existsSync(nodeRoot)
    ? readdirSync(nodeRoot, { recursive: true })
        .map(String)
        .filter((file) => file.endsWith(".ts"))
        .map((file) => join(nodeRoot, file))
    : []),
];
const source = authoringFiles.map((file) => readFileSync(file, "utf8")).join("\n");
for (const forbidden of [
  /Object\.fromEntries/,
  /\.reduce\s*\(/,
  /defaultSize/,
  /src\/catalog/,
  /src\/render\/theme/,
  /BUILTIN_DESCRIPTORS/,
  /LEGACY_COMPOSITION/,
  /defineFxNodeComposition/,
])
  if (forbidden.test(source))
    throw new Error(`Application composition is derived through a forbidden adapter: ${forbidden}`);
structuredClone(APPLICATION_COMPILED.source);
console.log(
  `application composition: ${ids.length} unique, materializable nodes (version ${APPLICATION_COMPILED.source.version})`,
);
