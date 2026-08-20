# Scene and shared data

Think of every handle as an array index, not an object mirrored into core. `Node.position`, `Node.quaternion`, and `Node.scale` are views into separate flat SOA arrays.

## Direct transform movement

```ts
import { Node } from "@yawn/handles";

const pivot = new Node(scene, { position: [0, 1, 0] });
await pivot.ready;

canvas.addEventListener("pointermove", (event) => {
  pivot.position[0] += event.movementX * 0.002;
  pivot.position[1] -= event.movementY * 0.002;
});
```

The pointer handler sends no messages. The typed-array view points directly into the arena shared with the render worker. The camera helpers use this same pattern; see [Cameras and controls](/guide/cameras).

<Playground example="sab" />

## Add an application-specific row

```ts
const particles = await scene.ensureRows("particleVelocity", 10_000, 16, "f32");
particles.row(42).set([1, 0, 0, 0]);
```

Rows are 16-byte-stride-aligned and arena allocations are 64-byte aligned. Formats are `f32`, `u32`, or `i32`.

## Timing and render skipping

Core always creates `info` as:

```text
[deltaTime, frameCount, elapsedTime, targetFps, skipRender, 0, 0, 0]
```

```ts
const info = scene.array("info").row(0);
info[4] = 1; // keep timing, skip GPU work
info[4] = 0; // resume rendering
```

Use messages for rare control changes (`setFps`, graph updates, allocation); use SAB writes for existing hot state.

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
