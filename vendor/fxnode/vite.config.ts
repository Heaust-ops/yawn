import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const sourceRoot = fileURLToPath(new URL("./src", import.meta.url));
const inside = (root: string, path: string): boolean => {
  const value = relative(root, path);
  return value === "" || (!value.startsWith("..") && !value.startsWith("/"));
};
const enforceLibraryAlias = (): Plugin => ({
  name: "fxnode-enforce-library-alias",
  enforce: "pre",
  resolveId(source, importer) {
    if (!importer || source.startsWith("@lib/") || inside(sourceRoot, importer.split("?", 1)[0]!)) return null;
    // Vite rewrites module-worker URLs to root-relative /src requests before
    // resolving them, so only author-written relative imports are boundaries.
    const target = source.startsWith(".") ? resolve(dirname(importer.split("?", 1)[0]!), source) : undefined;
    if (target && inside(sourceRoot, target))
      throw new Error(`Modules outside src must import library code through @lib/: ${source}`);
    return null;
  },
});

export default defineConfig({
  base: "./",
  plugins: [enforceLibraryAlias()],
  resolve: { alias: { "@lib": sourceRoot } },
  server: {
    allowedHosts: [".onamp.dev", ".e2b.app"],
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
  build: {
    assetsInlineLimit: 0,
    lib: {
      entry: { index: "src/index.ts", headless: "src/headless.ts", "widgets/color-ramp": "src/widgets/color-ramp.ts" },
      formats: ["es"],
    },
    rollupOptions: { output: { entryFileNames: "[name].js" } },
  },
});
