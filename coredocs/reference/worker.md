# Worker message reference

All fields below are top-level beside `type` and `request`. Except for `init` and `upload-texture`, transfer lists are empty.

| `type` | Fields | Transfer list | Success `result` | Constraints / effects |
|---|---|---|---|---|
| `init` | `canvas: OffscreenCanvas`, `arenaBytes: number` | `[canvas]` | `{ buffer: SharedArrayBuffer, rows: Descriptor[] }` | Once only; initializes WASM/WebGPU. |
| `create-rows` | `name`, `rows`, `stride`, `format` | — | descriptor | Nonempty name; rows > 0; stride ≥16 and multiple of 16; format f32/u32/i32. Existing name may only retain format/stride and grow. |
| `create-rows-batch` | `rows: Array<{name,rows,stride,format}>` | — | descriptor array in input order | Nonempty. Sequential, **not transactional**: earlier creations can survive a later failure. Active GPU resources refresh once after successful batch. |
| `delete-rows` | `name` | — | `undefined` | Not `signals`, not active slots, not referenced by active graph. |
| `allocate-object` | `name` | — | `{ id, rows: descriptor }` | Not `signals`; may grow/relocate. |
| `delete-object` | `name`, `id: u32` | — | `undefined` | ID must currently be active; row is zeroed. |
| `compile-graph` | `serialized: string` | — | graph ID string | Parses and stores; same ID replaces previously stored graph. Does not activate it. |
| `switch-loadout` | `id` | — | `undefined` | Builds GPU resources and activates stored graph. Clears `bundleDirty`, sets `sabDirty`. |
| `upload-texture` | `name`, `mipLevel: u32`, `image: ImageBitmap` | `[image]` | `undefined` | Uploads immediately if active graph has the texture and caches source for future switches. Source extent must fit destination/mip. Sets dirty. |
| `delete-texture` | `name` | — | `undefined` | Deletes/closes cached mip sources; does not remove graph texture or mark dirty. |
| `play` | — | — | `undefined` | Enables render-loop ticks; resets last-time baseline. |
| `pause` | — | — | `undefined` | Stops loop updates/renders. |
| `set-fps` | `fps: u32` | — | `undefined` | 0 uncapped; maximum 1000. Does not itself dirty a frame. |
| `set-profiler` | `enabled` (boolean-coerced) | — | boolean | Result says timestamp queries are supported. Enables only when requested and supported. |

Rust/WASM numeric conversion applies to `u32` fields; callers should send finite nonnegative integers in range rather than rely on coercion.

## Profiler events

When enabled and supported, the worker polls every 250 ms and may send an unsolicited message with **no request field**:

```js
{
  type: "profile",
  stats: {
    frame: 42,
    milliseconds: 0.31,
    readbackMilliseconds: 4.8,
    adapter: "Adapter name · DeviceType · Backend",
    canvas: { width: 1280, height: 720 },
    passes: [{ name: "triangle", milliseconds: 0.31 }]
  }
}
```

One pass timing corresponds to a compiled execution; compatible adjacent render passes may be merged and labeled together. Samples are throttled and asynchronous, so they are diagnostics, not one event per frame. Disabling clears pending published statistics and the worker polling interval.

## Texture lifecycle

Create an `ImageBitmap`, then relinquish it to the worker:

```js
const image = await createImageBitmap(blob);
await request("upload-texture", { name: "albedo", mipLevel: 0, image }, [image]);
```

The name is a graph texture ID, not an arbitrary row name. Upload can precede graph activation because sources are cached. Non-transient textures with an unchanged descriptor can be retained across switches; cached levels are reapplied when needed. `delete-texture` only removes these cached sources.
