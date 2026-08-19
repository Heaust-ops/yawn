# Pipeline and loadout recipes

WGSL belongs to a graph package or your application. Core contains no scene program.

## 05 — Attach the default pipelines

The optional package exports plain declarations, so copy only the programs your graph uses.

```js
import { defaultPipelines } from "@yawn/default-pipelines";
import { RenderGraph } from "@yawn/render-graph-js";

const graph = new RenderGraph("default_programs", 1);
for (const pipeline of defaultPipelines.render) {
  graph.renderPipeline(pipeline);
}
for (const pipeline of defaultPipelines.compute) {
  graph.computePipeline(pipeline);
}
const ast = graph.ast();
```

## 06 — Supply a custom render pipeline

Put source and entry points in the graph declaration. The shader must honor the scene ABI expected by the executor that uses it.

```js
const shader = /* wgsl */ `
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;
struct Input {
  @location(0) position: vec3<f32>,
  @location(3) model_0: vec4<f32>,
  @location(4) model_1: vec4<f32>,
  @location(5) model_2: vec4<f32>,
  @location(6) model_3: vec4<f32>,
}
@vertex fn vertex_main(input: Input) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(
    input.model_0,
    input.model_1,
    input.model_2,
    input.model_3,
  );
  return view_projection * model * vec4(input.position, 1.0);
}
@fragment fn fragment_main() -> @location(0) vec4<f32> {
  return vec4(0.2, 0.7, 1.0, 1.0);
}`;

const ast = new RenderGraph("custom_render_program", 1)
  .renderPipeline({
    name: "scene",
    shader,
    vertexEntry: "vertex_main",
    fragmentEntry: "fragment_main",
    doubleSided: false,
  })
  .ast();
```

## 07 — Supply a compute pipeline

Dispatch dimensions are graph data and are allocated with the rest of the loadout.

```js
const shader = /* wgsl */ `
@compute @workgroup_size(8, 1, 1)
fn initialize() {}
`;

const ast = new RenderGraph("compute_program", 1)
  .computePipeline({
    name: "initialize",
    shader,
    entry: "initialize",
    dispatch: [4, 1, 1],
  })
  .ast();
```

## 08 — Compile and switch

Compile first, then activate the prepared ID. Drop a candidate when your surrounding transaction fails.

```js
import { loadGraph } from "@yawn/render-graph-js";

const compiled = await loadGraph(core, graph);
try {
  await core.switchCompiledGraph(compiled.compiledId);
} catch (error) {
  await core.dropCompiledGraph(compiled.compiledId).catch(() => {});
  throw error;
}
```

<Playground
  id="jso-graph"
  title="Compile and activate a pipeline loadout"
  description="The full preset contains external render/compute declarations and a transient resource graph."
/>
