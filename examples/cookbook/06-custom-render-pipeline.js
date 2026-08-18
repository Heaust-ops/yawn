import { RenderGraph } from "@yawn/render-graph-js";

export const simpleSceneShader = /* wgsl */ `
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;
struct Input {
  @location(0) position: vec3<f32>,
  @location(3) model_0: vec4<f32>,
  @location(4) model_1: vec4<f32>,
  @location(5) model_2: vec4<f32>,
  @location(6) model_3: vec4<f32>,
}
@vertex fn vertex_main(input: Input) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(input.model_0, input.model_1, input.model_2, input.model_3);
  return view_projection * model * vec4(input.position, 1.0);
}
@fragment fn fragment_main() -> @location(0) vec4<f32> {
  return vec4(0.2, 0.7, 1.0, 1.0);
}`;

/** Supply scene WGSL and entry points as graph data, never as core source. */
export function customRenderPipelineExample() {
  return new RenderGraph("custom_render_program", 1)
    .renderPipeline({
      name: "ground_plane",
      shader: simpleSceneShader,
      vertexEntry: "vertex_main",
      fragmentEntry: "fragment_main",
      doubleSided: false,
    })
    .ast();
}
