const isolationHeaders = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
};

export default {
  title: "Yawn Core Wire API",
  description: "Raw worker, shared-memory, and render-graph reference",
  cleanUrls: true,
  themeConfig: {
    nav: [
      { text: "Boot", link: "/guide/boot" },
      { text: "Shared memory", link: "/guide/shared-memory" },
      { text: "Render graphs", link: "/guide/render-graphs" },
      { text: "Wire reference", link: "/reference/worker" },
    ],
    sidebar: [
      {
        text: "Guides",
        items: [
          { text: "Raw-core overview", link: "/" },
          { text: "Boot and transport", link: "/guide/boot" },
          { text: "Shared memory", link: "/guide/shared-memory" },
          { text: "Render graphs", link: "/guide/render-graphs" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Worker messages", link: "/reference/worker" },
          { text: "Graph schema", link: "/reference/graph-schema" },
          { text: "Errors and lifecycle", link: "/reference/errors" },
        ],
      },
    ],
    search: { provider: "local" },
  },
  vite: {
    server: { headers: isolationHeaders },
    preview: { headers: isolationHeaders },
  },
};
