import { RenderGraph } from "@yawn/render-graph-js";

export const initializeShader = /* wgsl */ `
@compute @workgroup_size(8, 1, 1)
fn initialize() {}
`;

/** Declare a binding-free compute pass that runs before graph render passes. */
export function computePipelineExample() {
  return new RenderGraph("compute_program", 1)
    .computePipeline({
      name: "initialize",
      shader: initializeShader,
      entry: "initialize",
      dispatch: [4, 1, 1],
    })
    .ast();
}
