import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Yawn",
  description: "Worker-native WebGPU rendering with shared render data.",
  base: "/docs/",
  outDir: "../dist/docs",
  cleanUrls: true,
  head: [
    ["meta", { name: "theme-color", content: "#0d1117" }],
    ["link", { rel: "icon", href: "data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 64 64%22><rect width=%2264%22 height=%2264%22 rx=%2214%22 fill=%22%230d1117%22/><path d=%22M13 14h10l9 17 9-17h10L37 40v11H27V40z%22 fill=%22%23ed7946%22/></svg>" }],
  ],
  themeConfig: {
    logo: {
      light: "data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 42 42%22><rect width=%2242%22 height=%2242%22 rx=%229%22 fill=%22%2311161e%22/><path d=%22M8 9h7l6 11 6-11h7l-9 17v7h-8v-7z%22 fill=%22%23ed7946%22/></svg>",
      dark: "data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 42 42%22><rect width=%2242%22 height=%2242%22 rx=%229%22 fill=%22%2311161e%22/><path d=%22M8 9h7l6 11 6-11h7l-9 17v7h-8v-7z%22 fill=%22%23ed7946%22/></svg>",
    },
    nav: [
      { text: "Learn", link: "/guide/first-scene" },
      { text: "Packages", link: "/packages/" },
      { text: "Recipes", link: "/recipes/" },
      { text: "Playground", link: "/../playground/" },
    ],
    sidebar: [
      {
        text: "Get started",
        items: [
          { text: "Your first scene", link: "/guide/first-scene" },
          { text: "How Yawn fits together", link: "/guide/architecture" },
        ],
      },
      {
        text: "Package tutorials",
        items: [
          { text: "Package map", link: "/packages/" },
          { text: "Core and render data", link: "/packages/core" },
          { text: "Render graph frontends", link: "/packages/render-graph" },
          { text: "glTF import worker", link: "/packages/gltf-import" },
          { text: "Conventional handles", link: "/packages/mesh-handles" },
        ],
      },
      {
        text: "Recipes",
        items: [
          { text: "All recipes", link: "/recipes/" },
          { text: "Graph authoring", link: "/recipes/graph-authoring" },
          { text: "Pipelines and loadouts", link: "/recipes/pipelines" },
          { text: "Assets and render data", link: "/recipes/render-data" },
          { text: "Runtime interaction", link: "/recipes/runtime" },
        ],
      },
    ],
    socialLinks: [
      { icon: "github", link: "https://github.com/heaust-ops/yawn" },
    ],
    search: { provider: "local" },
    outline: { level: [2, 3] },
    editLink: {
      pattern: "https://github.com/heaust-ops/yawn/edit/feat/core/docs/:path",
      text: "Edit this page on GitHub",
    },
    footer: {
      message: "Core owns render data and render graphs. Addons own conveniences.",
      copyright: "Yawn is pre-1.0 software.",
    },
  },
});
