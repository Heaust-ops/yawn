import { defaultPipelines } from "@yawn/default-pipelines";
import { RenderGraph } from "@yawn/render-graph-js";

/** Copy the optional package's external programs into a graph AST. */
export function defaultPipelineExample() {
  const graph = new RenderGraph("default_programs", 1);
  for (const pipeline of defaultPipelines.render)
    graph.renderPipeline(pipeline);
  for (const pipeline of defaultPipelines.compute)
    graph.computePipeline(pipeline);
  return graph.ast();
}
