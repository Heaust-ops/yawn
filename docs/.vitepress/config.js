import { defineConfig } from "vitepress";

const headers = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
};

const isolation = {
  name: "cross-origin-isolation",
  configureServer(server) {
    server.middlewares.use((_, response, next) => {
      for (const [name, value] of Object.entries(headers)) {
        response.setHeader(name, value);
      }
      next();
    });
  },
};

export default defineConfig({
  title: "Yawn",
  description: "Shared render data and a render graph.",
  cleanUrls: true,
  themeConfig: {
    nav: [
      { text: "Architecture", link: "/" },
      { text: "Playground", link: "/playground" },
    ],
  },
  vite: {
    plugins: [isolation],
    worker: { format: "es" },
    server: { allowedHosts: true, headers },
    preview: { allowedHosts: true, headers },
  },
});
