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

Graph frontends serialize plain data to `(yawn-graph 1 ...)`. Named `after` edges preserve DAG fan-out; Rust sorts passes, detects cycles, culls unused declarations, plans compatible transient lifetimes, and allocates the active loadout.

The `Scene` addon is one such frontend. It is replaceable and has no privileged core API.
