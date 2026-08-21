# Shared rows and invalidation

## Descriptor and views

Every descriptor is:

```js
{ name, rows, stride, format, offset, bytes }
```

`offset` and `bytes` are byte units into the returned WASM `SharedArrayBuffer`; `bytes === rows * stride`. `format` is exactly `"f32"`, `"u32"`, or `"i32"`. `stride` is bytes per row, at least 16 and divisible by 16. Allocations begin on 64-byte boundaries. A row is homogeneous—the selected scalar type covers its entire stride.

```js
const constructors = { f32: Float32Array, u32: Uint32Array, i32: Int32Array };
function viewFor(buffer, d) {
  const Ctor = constructors[d.format];
  if (!Ctor) throw new Error("unsupported row format");
  return new Ctor(buffer, d.offset, d.bytes / Ctor.BYTES_PER_ELEMENT);
}
function rowFor(buffer, d, index) {
  if (!Number.isInteger(index) || index < 0 || index >= d.rows) throw new RangeError();
  const view = viewFor(buffer, d);
  const width = d.stride / view.BYTES_PER_ELEMENT;
  return view.subarray(index * width, (index + 1) * width);
}
```

The buffer stays the same, but a growing row array may relocate. Replace the cached descriptor—and recreate cached views—after every `create-rows`, batch result, and `allocate-object` result. `allocate-object` always returns `{ id, rows: descriptor }`, even without growth. Never infer an address from a prior descriptor.

## Slots

Creating rows establishes capacity but does not allocate objects. `allocate-object` returns a zero-based row ID. IDs increase until capacity is exhausted; allocation then grows to `id + 1`. Deleted IDs are kept in a LIFO free list and reused. `delete-object` zeroes the entire row and releases the ID. It rejects an inactive/duplicate ID. A row array cannot be deleted while any IDs are active, while `signals` cannot be allocated or deleted, and an array used by the active graph cannot be deleted.

Capacity does not shrink. Deletion makes descriptors/views invalid for application use even though stale bytes may remain in memory.

## Signals

`signals` is one `f32` row with 32-byte stride:

| Lane | Name | Meaning |
|---:|---|---|
| 0 | `deltaTime` | Seconds since the preceding accepted loop tick |
| 1 | `frameCount` | Frame counter represented as f32 |
| 2 | `elapsedTime` | Accumulated seconds |
| 3 | `targetFps` | Configured cap; 0 means uncapped |
| 4 | `skipRender` | Nonzero suppresses rendering |
| 5 | `sabDirty` | Nonzero requests a frame; consumed (set to 0) at frame start |
| 6 | `bundleDirty` | Nonzero suppresses stale bundles until a successful loadout switch |
| 7 | `reserved` | No current contract; leave unchanged/zero |

The core updates lanes 0–3 while playing. A render starts only when lanes 4 and 6 are zero and lane 5 is nonzero. Finish the row writes first, then set the dirty signal:

```js
const signalsF32 = rowFor(buffer, signalDescriptor, 0);

function publishRows(write) {
  write();           // finish every row write
  signalsF32[5] = 1; // request the frame last
}

async function replaceBundle(mutate, compileAndSwitch) {
  signalsF32[6] = 1; // suppress the old bundle first
  mutate();
  signalsF32[5] = 1;
  try {
    await compileAndSwitch(); // successful switch clears lane 6 and requests a frame
  } catch (error) {
    // Keep lane 6 set: rendering stale bindings would be unsafe. Repair/retry or stop.
    throw error;
  }
}
```

The signals are invalidation state, not a lock or transaction boundary. If multiple threads can write one logical update, coordinate those writers so core cannot observe a partially updated value. Core marks lane 5 after object deletion, texture upload, and a successful switch, but direct SAB writes do not. Always publish lane 5 after direct data writes.

## Frame-sync versus loadout-sync

A graph buffer's `sync` defaults to `"frame"`. Before each actual render, the full named row array is copied SAB → GPU, so ordinary value edits only require `sabDirty`.

`sync: "loadout"` is copied only when GPU resources are activated: on `switch-loadout`, and when core refreshes an active graph because a used row array grows/is recreated. Direct edits afterward do **not** reach that GPU buffer. To change loadout-synced content reliably, set `bundleDirty`, mutate, and switch a compiled replacement loadout (the replacement may use a newly compiled graph with the same schema). Structural changes affecting buffer size, bindings, offsets, draw ranges, passes, pipelines, or attachment/resource declarations also require bundle invalidation and a graph/loadout switch. This distinction matters because render commands are precompiled into render bundles.
