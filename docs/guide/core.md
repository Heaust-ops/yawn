# Core boundary

`@yawn/core` deliberately contains no scene types and no WGSL. It owns two things:

1. a 64-byte-aligned arena of named f32/u32/i32 SOA rows in one `SharedArrayBuffer`;
2. render-graph compilation, up-front WebGPU loadouts, transient resource aliasing, render bundles, and the paced render loop.

```text
infrequent worker messages                  hot shared mutations
┌──────────────────────────────┐            ┌──────────────────────┐
│ create/delete rows           │            │ transforms           │
│ allocate/delete object slot  │            │ cameras/materials    │
│ compile/switch graph         │            │ lights/app data      │
│ play/pause/set FPS           │            │ info.skipRender      │
└──────────────┬───────────────┘            └──────────┬───────────┘
               └─────────────────┬─────────────────────┘
                                 ▼
                        ┌─────────────────┐
                        │ Rust/WASM core  │
                        └─────────────────┘
```

## Use core directly

```ts
import { YawnCore } from "@yawn/core";

const core = new YawnCore(canvas, { arenaBytes: 64 * 1024 * 1024 });
await core.ready;

const values = await core.createRows({
  name: "application.values",
  rows: 1024,
  stride: 16,
  format: "f32",
});

const id = await core.allocateObject("application.values");
values.row(id).set([1, 2, 3, 4]);
```

Graph frontends serialize plain data to `(yawn-graph 1 ...)`. Named `after` edges preserve DAG fan-out; Rust sorts passes, detects cycles, culls unused declarations, aliases compatible transient lifetimes, merges compatible render passes into bundles, and allocates the active loadout. Unchanged persistent textures survive loadout rebuilds.

The `Scene` addon is one such frontend. It is replaceable and has no privileged core API.

## Profile physical GPU passes

```ts
const stop = core.onProfile((frame) => {
  console.table(frame.passes); // name + GPU milliseconds
});
const supported = await core.setProfiler(true);

// Later:
await core.setProfiler(false);
stop();
```

`new YawnCore(canvas, { debug: true })` enables the same timestamp-query mode at startup. Timings describe physical GPU passes and actual compiled draw counts; compatible indexed draws over consecutive instances collapse into one command. The sidebar also reports canvas size and wall-clock completion time.

The saved **Forward benchmark** playground renders 138 logical objects and 4.5 million triangles. Add `&grid=32` to lower geometry density or `&overdraw=1` to stack the objects while diagnosing depth and fragment cost.

<Playground example="core" />

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
