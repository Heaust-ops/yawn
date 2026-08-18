import { graphFromObject } from "@yawn/render-graph-js";

/** Author a graph with an ordinary JavaScript object and receive canonical AST. */
export function jsoGraphExample() {
  return graphFromObject({
    id: "jso_graph",
    revision: 1,
    nodes: [
      {
        id: "mesh",
        state: "enabled",
        executor: { key: "mesh", version: 2 },
        parameters: {},
        inputs: {},
      },
    ],
  });
}
