# Compute passes

Compute shaders are graph data, not core code. Declare the shared rows/resources a shader needs and attach the pass to `Scene`.

```ts
const velocity = await scene.ensureRows("velocity", 4096, 16, "f32");

const simulation = new ComputePass({
  id: "integrate",
  code: `
    @group(0) @binding(0)
    var<storage, read_write> velocity: array<vec4<f32>>;

    @compute @workgroup_size(64)
    fn main(@builtin(global_invocation_id) id: vec3<u32>) {
      if (id.x < arrayLength(&velocity)) {
        velocity[id.x].y -= 0.001;
      }
    }
  `,
  buffers: [{ id: "velocity", array: "velocity", usage: ["storage"] }],
  bindings: [{ group: 0, binding: 0, resource: "velocity" }],
  dispatch: [64, 1, 1],
});

await scene.addComputePass(simulation);
```

<Playground example="compute" />

The graph DAG places an unqualified custom compute pass after light clustering and makes forward rendering depend on all custom compute passes. Use `after` to specify other dependencies.

```ts
await simulation.update({ dispatch: [128, 1, 1] });
await scene.removeComputePass(simulation);
```

Both operations are infrequent graph/loadout messages. Existing source row changes are direct SAB writes.

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
