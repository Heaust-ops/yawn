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
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Core", link: "/guide/core" },
      { text: "Playground", link: "/playground" },
    ],
    sidebar: {
      "/guide/": [
        {
          text: "Learn Yawn",
          items: [
            { text: "Getting started", link: "/guide/getting-started" },
            { text: "Scene and shared data", link: "/guide/scene-and-sab" },
            { text: "Cameras and controls", link: "/guide/cameras" },
            { text: "Meshes and instances", link: "/guide/meshes-and-instances" },
            { text: "Materials and textures", link: "/guide/materials" },
            { text: "Clustered lights", link: "/guide/lights" },
            { text: "Compute passes", link: "/guide/compute" },
            { text: "Post processing", link: "/guide/post-processing" },
            { text: "glTF and picking", link: "/guide/importing-and-picking" },
            { text: "Core boundary", link: "/guide/core" },
          ],
        },
      ],
    },
  },
  vite: {
    plugins: [isolation],
    worker: { format: "es" },
    server: { allowedHosts: true, headers },
    preview: { allowedHosts: true, headers },
  },
});
