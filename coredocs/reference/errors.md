# Errors and lifecycle

Worker failures are string codes in `{ request, error }`. GPU/browser validation can also produce implementation-originated error messages rather than a stable code; preserve the complete string in logs while branching only on documented codes.

## Transport and initialization

| Code | Meaning / response |
|---|---|
| `INIT` | Duplicate init, wrong canvas type, or invalid arena capacity. Start a fresh worker/canvas or fix capacity. |
| `WASM_MEMORY_NOT_SHARED` | WASM was built/deployed without shared memory or isolation failed. |
| `UNINITIALIZED` | Non-init request arrived before successful init. Serialize boot. |
| `MESSAGE` | Unknown/missing message type. |
| `CORE_ERROR` | Fallback when the thrown value has no message. Treat as fatal/unknown. |
| `WEBGPU_UNINITIALIZED` | A GPU operation occurred without initialized WebGPU. |

`WORKER_ERROR` and `DISPOSED` are wrapper conventions, not messages emitted by `worker.js`; a raw client should use equivalent local codes when rejecting pending work.

## Rows, arena, and objects

| Code | Meaning |
|---|---|
| `ROWS` | Invalid row shape/format, incompatible recreation, empty batch, or malformed batch payload. |
| `ROWS_BUILTIN` | Attempt to grow/delete/allocate from `signals`. |
| `ROWS_UNKNOWN` | Row name does not exist (also possible during frame upload). |
| `ROWS_ACTIVE` | Delete blocked by active object IDs or active graph reference. |
| `ARENA_OOM` | Byte multiplication overflow or no aligned free arena block. |
| `OBJECT_LIMIT` | Slot ID cannot increment past u32. |
| `OBJECT_UNKNOWN` | Delete ID is not active for that row array. |
| `FPS` | FPS exceeds 1000. |

On row growth, consume the returned descriptor before the next access. Batch creation is not rollback-safe. Do not retry allocation blindly after an uncertain transport outcome: it may allocate a second slot.

## Graph parse and planning

| Code | Meaning |
|---|---|
| `GRAPH_WIRE` | Invalid S-expression/tag/version, duplicate field, or unsupported wire value. |
| `GRAPH_SHAPE` | Decoded object cannot deserialize, graph ID empty, or no passes. |
| `GRAPH_PASS` | Empty/duplicate pass ID or invalid pass type. |
| `GRAPH_DEPENDENCY` / `GRAPH_CYCLE` | Missing `after` ID / dependency cycle. |
| `GRAPH_RESOURCE` | Empty/duplicate resource ID or texture planning serialization failure. |
| `GRAPH_PIPELINE` | Empty/duplicate/missing or wrong-kind pipeline. |
| `GRAPH_BUFFER_SYNC` | Buffer sync is not `frame` or `loadout`. |
| `GRAPH_UNKNOWN` | Switch ID has not been compiled/stored. |
| `GRAPH_ARRAY_UNKNOWN` | Buffer declaration names no existing row array at activation. |
| `GRAPH_RESOURCE_UNKNOWN` | Pass binding or vertex/index resource cannot be resolved. |
| `GRAPH_ATTACHMENT` | Attachment is neither `canvas` nor a declared active texture. |

## GPU descriptor/value errors

| Codes | Meaning |
|---|---|
| `GRAPH_BUFFER_USAGE`, `GRAPH_TEXTURE_USAGE` | Unknown usage string. |
| `GRAPH_TEXTURE_SIZE`, `GRAPH_TEXTURE_DIMENSION`, `GRAPH_TEXTURE_FORMAT` | Invalid extent, dimension, or texture format. |
| `GRAPH_TEXTURE_UNKNOWN` | Upload target absent from active resources (normally only reached for an active upload path). |
| `GRAPH_VERTEX_FORMAT`, `GRAPH_VERTEX_STEP`, `GRAPH_INDEX_FORMAT` | Invalid vertex layout/index enum. |
| `GRAPH_PRIMITIVE`, `GRAPH_DEPTH_STENCIL`, `GRAPH_MULTISAMPLE` | Invalid pipeline descriptor. |
| `GRAPH_BLEND`, `GRAPH_WRITE_MASK` | Invalid fragment blend or write-mask bits. |
| `GRAPH_SAMPLER` | Invalid sampler enum/compare value. |
| `GRAPH_LOAD_OP`, `GRAPH_STORE_OP` | Attachment operation is not allowed. |
| `SURFACE` | Surface acquisition/recovery failed; core marks the SAB dirty to retry later. |

Shader compilation, bind-layout mismatch, device limits, invalid upload extents/mips, and WebGPU validation are not all normalized to these codes. Validate graph inputs before switching and retain the previous graph source for recovery.

## Safe state transitions

1. Initialize once and cache descriptors.
2. Create rows, allocate IDs, and initialize complete rows before publishing `sabDirty`.
3. Compile graphs before switching. Compilation stores a graph but does not validate every GPU-dependent property; activation can still fail.
4. For bundle-invalidating mutation, set `bundleDirty` first. Leave it set after compile/switch failure, because that intentionally suppresses stale rendering. Repair and switch successfully; never clear it merely to hide an error.
5. Upload textures with transfer ownership. Delete cached sources when no future loadout needs them.
6. Pause when editor state should stop timing/render-loop progress. `pause` does not cancel requests or clear dirty lanes.
7. On worker failure, reject all pending requests and rebuild the entire worker/canvas/core state. Requests are not generally safe to replay because create, allocate, delete, switch, and texture operations may already have committed.
