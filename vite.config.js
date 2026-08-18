import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { ViteRsw } from "vite-plugin-rsw";
import wasm from "vite-plugin-wasm";
import { cp, mkdir } from "node:fs/promises";
import { cpSync, mkdirSync } from "node:fs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const portalHost = process.env.PUBLIC_URL
  ? new URL(process.env.PUBLIC_URL).hostname
  : undefined;
const serverPort = Number(process.env.PORT) || 8080;
const wasmSource = path.resolve(__dirname, "level-editor/pkg");
const exampleRoot = path.resolve(__dirname, "examples/render-graph-studio");
const wasmDestination = path.resolve(exampleRoot, "level-editor/pkg");

// Vite resolves entry imports before plugin build hooks. Mirror synchronously while
// loading the config so a clean build always sees the complete generated package.
mkdirSync(wasmDestination, { recursive: true });
cpSync(wasmSource, wasmDestination, { recursive: true, force: true });

function freshWasmPackage() {
  let copying, timer;
  const mirror = () => copying ??= mkdir(wasmDestination, { recursive: true })
    .then(() => cp(wasmSource, wasmDestination, { recursive: true, force: true }))
    .finally(() => { copying = undefined; });
  return {
    name: "fresh-wasm-package",
    async buildStart() { await mirror(); },
    configureServer(server) {
      server.watcher.add(wasmSource);
      const update = file => {
        if (!file.startsWith(wasmSource)) return;
        clearTimeout(timer);
        // rsw replaces several package files; mirror only after that write burst settles.
        timer = setTimeout(() => mirror().then(() => server.ws.send({ type: "full-reload" })), 100);
      };
      server.watcher.on("add", update).on("change", update).on("unlink", update);
    },
  };
}

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        app: path.resolve(exampleRoot, "index.html"),
      },
    },
    // Relative to 'root'.
    outDir: "../../dist",
    copyPublicDir: true,
  },
  // Build output stays at the repository root, outside the nested example root.
  root: exampleRoot,
  // The example is source code, not Vite's untransformed public directory. Treating it
  // as both makes dev mode reject the generated WASM worker's module imports.
  publicDir: false,
  worker: {
    format: "es",
  },
  resolve: {
    alias: {
      "@fxnode/": `${path.resolve(__dirname, "vendor/fxnode/src")}/`,
      "@yawn/render-graph-ast": path.resolve(
        __dirname,
        "addons/render-graph-ast/src/index.js",
      ),
      "@yawn/render-graph-js": path.resolve(
        __dirname,
        "addons/render-graph-js/src/index.js",
      ),
      "@yawn/render-graph-fxnode/catalog": path.resolve(
        __dirname,
        "addons/render-graph-fxnode/src/catalog.js",
      ),
      "@yawn/render-graph-fxnode": path.resolve(
        __dirname,
        "addons/render-graph-fxnode/src/index.js",
      ),
      "@yawn/core/snapshot": path.resolve(
        __dirname,
        "packages/yawn-core/src/snapshot.js",
      ),
      "@yawn/core": path.resolve(__dirname, "packages/yawn-core/src/index.js"),
      "@yawn/mesh-handles": path.resolve(
        __dirname,
        "addons/mesh-handles/src/index.js",
      ),
      "@yawn/gltf-import": path.resolve(
        __dirname,
        "addons/gltf-import/src/index.js",
      ),
      "@yawn/default-pipelines": path.resolve(
        __dirname,
        "addons/default-pipelines/src/index.js",
      ),
      pkg: path.resolve(__dirname, "level-editor/pkg"),
    },
  },
  plugins: [
    freshWasmPackage(),
    ViteRsw(),
    // Makes us be able to use top level await for wasm.
    // Otherwise, we can restrict build.target to 'es2022', which allows top level await.
    wasm(),
  ],
  server: {
    host: process.env.PORT ? "0.0.0.0" : undefined,
    port: serverPort,
    strictPort: true,
    allowedHosts: portalHost ? [portalHost, ".e2b.app"] : [],
    headers: {
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Opener-Policy": "same-origin",
    },
    fs: {
      strict: false,
    },
  },
  preview: {
    port: 8080,
    allowedHosts: portalHost ? [portalHost, ".e2b.app"] : [],
    headers: {
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Opener-Policy": "same-origin",
    },
  },
});
