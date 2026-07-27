import { defineConfig } from "vitepress";
import generated from "../reference/generated/typedoc-sidebar.json" with { type: "json" };

const learn = [
  { text: "Learn", link: "/learn/" },
  {
    text: "Tutorials",
    collapsed: false,
    items: [
      { text: "Overview", link: "/learn/tutorials/" },
      { text: "Your first node", link: "/learn/tutorials/first-node" },
      { text: "Color Balance", link: "/learn/tutorials/color-balance" },
      { text: "Live composition", link: "/learn/tutorials/live-composition" },
    ],
  },
  {
    text: "Concepts",
    items: [
      { text: "Overview", link: "/learn/concepts/" },
      { text: "Worker authority", link: "/learn/concepts/worker-authority" },
      { text: "Composition", link: "/learn/concepts/composition" },
      { text: "Graph state and events", link: "/learn/concepts/graph-state-and-events" },
      { text: "State and persistence", link: "/learn/concepts/state-and-persistence" },
    ],
  },
  {
    text: "Guides",
    items: [
      { text: "Overview", link: "/learn/guides/" },
      { text: "Browser host", link: "/learn/guides/browser-host" },
      { text: "Interactions", link: "/learn/guides/interactions" },
      { text: "Rendering and lifecycle", link: "/learn/guides/rendering-and-lifecycle" },
      { text: "Browser support", link: "/learn/guides/browser-support" },
      { text: "CSP", link: "/learn/guides/csp" },
      { text: "Accessibility", link: "/learn/guides/accessibility" },
    ],
  },
  { text: "Examples", link: "/learn/examples/" },
];

export default defineConfig({
  title: "fxnode",
  description: "A typed, worker-owned node editor",
  lastUpdated: true,
  srcExclude: ["research/**", "decisions/**"],
  ignoreDeadLinks: false,
  transformPageData(pageData) {
    if (pageData.relativePath.startsWith("reference/generated/"))
      pageData.frontmatter = { ...pageData.frontmatter, editLink: false, lastUpdated: false };
  },
  themeConfig: {
    nav: [
      { text: "Learn", link: "/learn/" },
      { text: "API Reference", link: "/reference/" },
    ],
    sidebar: { "/learn/": learn, "/reference/": [{ text: "API Reference", link: "/reference/" }, ...generated] },
    search: { provider: "local" },
    outline: { level: [2, 3] },
    editLink: {
      pattern: "https://github.com/Heaust-ops/fxnode/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },
    socialLinks: [{ icon: "github", link: "https://github.com/Heaust-ops/fxnode" }],
    footer: { message: "Released under the MIT License.", copyright: "Copyright © fxnode contributors" },
  },
});
