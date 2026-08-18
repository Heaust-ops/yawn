import { RenderGraph, ref } from "@yawn/render-graph-js";

/** Build the same sort of DAG with the small mutable authoring facade. */
export function fluentGraphExample() {
  return new RenderGraph("fluent_graph", 1)
    .node("source", "and", { version: 2 })
    .node("consumer", "not", {
      version: 1,
      inputs: { operand: [ref("source", "value")] },
    })
    .ast();
}
