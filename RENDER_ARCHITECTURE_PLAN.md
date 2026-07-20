# Data-Driven Render Architecture Plan

## Status

This document is the implementation plan for replacing the current per-mesh,
per-buffer renderer with a data-driven renderer owned by Rust/Wasm.

The architecture is intentionally staged. The canonical data model, handles,
frame commit protocol, and GPU mirror must be established before auxiliary
workers and advanced culling are added.

## Goals

- Rust/Wasm is the sole owner of canonical render state.
- JavaScript `Mesh` objects contain only thin, stable handles.
- Geometry, instance, pipeline, and render metadata are stored in SoA data.
- New geometry creates one mesh instance by default.
- Shared memory avoids per-property JS-to-Wasm calls.
- Main-thread mutations are deduplicated and committed at most once per render
  cycle.
- Large GPU buffers replace per-mesh GPU buffer allocations.
- Configurable stores double automatically until a hard budget or device limit
  is reached.
- File and directory handles are read by a dedicated I/O worker.
- A dedicated bounds worker maintains geometry-local AABBs.
- A dedicated BVH worker builds scene spatial data for picking and other
  CPU-side spatial queries.
- A compute shader performs render frustum culling and later conservative
  occlusion culling.
- The draw hot path consumes prevalidated, resolved draw packets without
  repeated geometry, pipeline, or handle checks.

## Non-goals for the initial implementation

- CPU or GPU arena compaction.
- Arbitrary user-defined pipelines through the public JavaScript API.
- In-place shared geometry editing.
- Multi-producer command queues.
- Multi-draw indirect as a required baseline capability.
- Exact triangle picking as part of the scene BVH.
- File watching or assuming that filesystem permissions are permanent.
- Persistent browser-private caching such as OPFS.
- Automatic eviction of geometry that has no recoverable source.

## System overview

```text
┌────────────────────────── Main JavaScript ──────────────────────────┐
│                                                                    │
│  Mesh { renderer, slot, generation }                               │
│                                                                    │
│  PendingMutationJournal                                            │
│    ├── coalesced absolute assignments                              │
│    └── ordered lifecycle/resource barriers                         │
│                                                                    │
│  File/directory pickers                                            │
└──────────────┬───────────────────────────────┬─────────────────────┘
               │ one credited batch/cycle      │ cloned handles
               ▼                               ▼
┌──────────────────────────────┐   ┌───────────────────────────────┐
│ Rust/Wasm render worker      │◀──│ Dedicated filesystem worker  │
│                              │   │                               │
│ Canonical RenderData         │   │ Async handle reads           │
│ GPU mirror                   │   │ Bounded payload delivery      │
│ Frame commit coordinator     │   │ Cancellation/backpressure     │
└─────────┬───────────┬────────┘   └───────────────────────────────┘
          │           │
          │ sealed    │ coherent SpatialSnapshot
          │ geometry  │
          ▼           ▼
┌──────────────────┐  ┌─────────────────────────────────────────────┐
│ Bounds worker    │  │ Scene BVH worker                            │
│                  │  │                                             │
│ Local AABB SoA   │  │ World AABBs and immutable scene BVH         │
└────────┬─────────┘  └───────────────────┬─────────────────────────┘
         │ versioned result                │ versioned result
         └──────────────────▶ Render worker ◀───────────────────────┘
                                      │
                                      ▼
                         GPU culling compute pass
                                      │
                                      ▼
                         Indirect render commands
```

## Ownership model

### Canonical state

The render worker is the only writer of canonical state:

```rust
pub struct RenderData {
    pub geometry: GeometryStore,
    pub instances: InstanceSoa,
    pub pipelines: PipelineTable,
    pub dirty: DirtySets,
    pub config: RenderDataConfig,
}
```

All public mutations are commands. Auxiliary workers may publish derived
results, but those results do not become accepted renderer state until the
render worker validates and commits them at a frame boundary.

### Derived state

Derived state has explicit single-writer ownership:

- The bounds worker writes unpublished local-AABB results.
- The BVH worker writes unpublished world bounds and BVH buffers.
- The GPU compute pass writes visibility and indirect draw outputs.
- The render worker accepts or rejects worker results and publishes the
  current committed descriptors.

No shared region has multiple writers.

## Geometry and mesh instances

Geometry and mesh instances are separate resources.

### Geometry

Geometry contains immutable or replacement-based asset data:

- Positions.
- Normals.
- UVs.
- Indices.
- Local bounds and bounds version.
- Vertex/index formats.
- CPU and GPU residency metadata.
- Recoverable source identity when loaded from a file handle.

A glTF primitive creates one geometry resource. Geometry remains in primitive
local space; node transforms are not baked into positions or normals.

### Mesh instance

A mesh instance contains render-specific state:

- Geometry handle.
- World transform.
- Pipeline key.
- Material key when material support is added.
- Visibility and render flags.
- Layer and selection masks.
- Dirty versions.

Creating raw geometry creates exactly one instance by default. Additional
instances share the immutable geometry.

## Stable virtual handles

JavaScript must never retain Rust allocation addresses, `Vec` pointers,
physical geometry offsets, or GPU buffer indices.

Public resources use logical generational handles:

```rust
#[repr(C)]
pub struct MeshHandle {
    pub slot: u32,
    pub generation: u32,
}

#[repr(C)]
pub struct GeometryHandle {
    pub slot: u32,
    pub generation: u32,
}
```

Rules:

- Generation zero is invalid.
- Generation increments when a destroyed slot is reused.
- A slot is retired rather than allowing generation wrap to cause ABA.
- Array growth never changes a live handle.
- Geometry eviction or reload does not change generation.
- Resource kinds are distinct and cannot validate as one another.
- Every external command validates slot and generation at commit time.
- A JavaScript wrapper is tombstoned immediately after `destroy()` is called.

Two `u32` values are used instead of a JavaScript `Number`-encoded `u64` so
handle values remain exact and map directly to the binary command ABI.

## Canonical SoA stores

### Instance SoA

The instance store uses stable logical slots. Every column has the same slot
capacity.

```rust
pub struct InstanceSoa {
    generations: Vec<u32>,
    occupied: BitSet,

    geometry_slots: Vec<u32>,
    geometry_generations: Vec<u32>,

    transforms: Vec<Mat4>,
    pipeline_keys: Vec<PipelineKey>,
    material_keys: Vec<MaterialKey>,
    render_flags: Vec<RenderFlags>,
    layer_masks: Vec<u32>,

    transform_versions: Vec<u32>,
    state_versions: Vec<u32>,
}
```

Initial render flags include:

- `VISIBLE`.
- `CASTS_SHADOW`.
- `RECEIVES_SHADOW`.
- `SELECTABLE`.
- `ALWAYS_VISIBLE` for conservative culling fallback.
- Internal dirty flags where a separate dirty bitset is not preferable.

`shouldRender` is derived rather than stored redundantly. An instance enters
the ready set only when it is occupied, visible, and has all required resolved
GPU and pipeline resources.

### Geometry store

The geometry store presents a logical global SoA of positions, normals, UVs,
and indices. CPU storage is paged or chunked behind a geometry allocation
table rather than implemented as one permanently contiguous `Vec`.

```text
GeometryHandle
      │
      ▼
GeometryRecord
  ├── content version
  ├── vertex/index counts and formats
  ├── immutable page/span descriptors
  ├── bounds state and accepted bounds version
  ├── CPU residency/source state
  ├── GPU allocation state
  └── geometry-to-instance reverse references
```

Paging permits whole geometry allocations to be released without moving every
other resource. It also permits immutable snapshot leases for the bounds
worker.

Geometry is immutable in the first implementation. Editing creates a new
content version or replacement allocation. Mutable geometry may later use
copy-on-write pages.

### Pipeline table

Instances store a validated `PipelineKey`, not raw strings, arbitrary JS
integers, or `wgpu::RenderPipeline` references.

```rust
pub struct PipelineRecord {
    pub key: PipelineKey,
    pub gpu_pipeline_index: usize,
    pub render_phase: RenderPhase,
    pub vertex_schema: VertexSchema,
    pub flags: PipelineFlags,
    pub readiness: PipelineReadiness,
}
```

A complete pipeline key includes every state dimension required for pipeline
and binding compatibility, not only shader identity.

## Capacity growth and budgets

Growable stores have configurable initial capacity, current capacity, and a
hard limit.

```text
new capacity = max(required capacity, current capacity × 2)
```

Doubling applies to:

- Geometry slot metadata.
- Instance SoA columns.
- CPU geometry page pools.
- GPU vertex, index, and instance buffers.
- JS read projections.

Command and completion rings are fixed after startup so their atomic protocol
does not need dynamic relocation.

Growth is rejected when it would exceed:

- The configured decoded-geometry budget.
- The configured Wasm high-water limit.
- The configured GPU budget, including transient old-plus-new allocations.
- WebGPU device limits such as maximum buffer size.
- Checked vertex/index/count arithmetic.

Wasm linear memory cannot shrink. Releasing decoded geometry limits live
allocations but does not promise that the browser process returns to an earlier
linear-memory size. The allocator must prevent growth beyond a configured
high-water mark rather than relying on later release to shrink memory.

## Shared-memory ABI

A `SharedArrayBuffer` cannot be technically read-only. The supported API makes
the public projection read-only by contract, while all supported mutations use
the command journal. Rust still validates external handles and commands.

### Stable ABI root

Rust allocates one pinned ABI root and never moves or frees it:

```rust
#[repr(C)]
pub struct AbiRoot {
    magic: u32,
    abi_version: u32,
    root_sequence: AtomicU32,
    layout_epoch: AtomicU32,

    instance_projection_offset: AtomicU32,
    instance_projection_capacity: AtomicU32,
    instance_projection_stride: u32,

    command_ring_offset: u32,
    completion_ring_offset: u32,
    frame_credit_offset: u32,
}
```

Internal Rust `Vec` storage is never part of the ABI. When a projection moves,
Rust publishes its new offset and capacity and advances the layout epoch.
JavaScript reacquires `memory.buffer` and creates fresh typed-array views rather
than retaining views indefinitely.

Multiword values such as transforms use a per-slot sequence so JavaScript
observes either the complete previous value or the complete next value.

### JavaScript read projection

The public projection contains only Mesh API metadata:

- Generation and state.
- Transform.
- Local or world bounds where exposed.
- Pipeline key.
- Render flags.
- Geometry counts and status.
- Request/load status.

Large geometry arrays remain private in the initial API.

## JavaScript Mesh API

A JavaScript mesh contains only its renderer facade and logical handle:

```js
class Mesh {
  #renderer;
  #slot;
  #generation;

  get visible() {
    return this.#renderer.readVisible(this.#slot, this.#generation);
  }

  set visible(value) {
    this.#renderer.stageVisible(this.#slot, this.#generation, value);
  }

  setTransform(matrix) {
    return this.#renderer.stageTransform(
      this.#slot,
      this.#generation,
      matrix,
    );
  }

  destroy() {
    return this.#renderer.stageDestroy(this.#slot, this.#generation);
  }
}
```

Recommended initial API behavior:

```js
const meshes = await renderer.loadGltf(fileHandle);

const mesh = await renderer.createMesh({
  positions,
  normals,
  uvs,
  indices,
  pipeline: "gltf_standard",
});

const clone = await mesh.clone();
mesh.visible = false;
await mesh.setTransform(matrix);
await mesh.destroy();
```

`loadGltf()` returns one `Mesh` per imported node/primitive instance. A future
`Model` aggregate may group returned meshes without changing their identities.

## Render-cycle mutation gate

Main JavaScript does not publish every setter immediately. It accumulates an
ordered mutation journal until the render worker grants the next commit credit.

### Coalescing segments

Absolute assignments are deduplicated by:

```text
mesh slot + mesh generation + field
```

The last assignment in a segment wins for:

- Transform.
- Visibility.
- Pipeline selection.
- Material selection.
- Complete render-flag values.
- Layer and selection masks.

Relative operations such as translation by a delta, toggles, increments, or
partial bit operations remain ordered unless the JS API normalizes them to an
absolute value before staging.

### Ordering barriers

Lifecycle and resource operations seal the current coalescing segment and form
an ordering barrier:

- Create.
- Destroy.
- Clone.
- Load or unload.
- Geometry replacement.
- Pipeline-resource lifecycle changes.

This preserves ordering such as `SetTransform(handle)` followed by
`Destroy(handle)`.

### One outstanding credit

The render worker issues one frame-credit token. Main JS may submit at most one
complete batch for that token.

- Credits do not accumulate.
- The renderer never waits for JavaScript to use a credit.
- Mutations arriving after a flush snapshot remain pending for the next token.
- A submitted batch is committed even if presentation is paused or hidden.
- If the complete batch cannot fit, none of it is published.
- Lifecycle commands are never partially submitted or dropped.
- Duplicate, consumed, or unknown tokens are rejected.

The worker acknowledges the consumed token and issues the next one after the
batch has been committed and its projection/completions have been published.

### Command ring

The initial command ring is SPSC: the browser main agent is the producer and
the render worker is the consumer.

Each fixed-size command record includes:

```text
ABI version
opcode
record size
request ID
target slot
target generation
payload
```

The producer writes a complete payload before atomically publishing the head.
The consumer reads only published records and atomically releases consumed
slots. The browser main thread never spins or blocks with `Atomics.wait()`.

A sealed batch is either copied into worker-owned staging before semantic
processing or remains immutable until acknowledgement.

### Completion ring

The render worker publishes:

- Created handles.
- Mutation completion or rejection.
- Load progress and completion.
- Capacity and budget failures.
- Permission/source failures.
- Worker and device errors.

The committed JS projection is published before completion promises resolve.
After a mutation promise resolves, getters therefore observe that mutation or
a newer committed state.

## Filesystem worker

There is no OPFS cache. User-selected `FileSystemFileHandle` and
`FileSystemDirectoryHandle` capabilities are handled by a dedicated I/O
worker.

### Access flow

1. The JavaScript Mesh/load API initiates a picker under user activation.
2. Main JavaScript receives the selected handle.
3. The handle is structured-cloned, not transferred, to the I/O worker.
4. The I/O worker checks current permission and reads asynchronously.
5. A dedicated `MessageChannel` delivers payloads directly to the render
   worker without relaying bulk bytes through main JavaScript.
6. A mutation-journal load command authorizes committing the payload into
   `RenderData`.

The I/O worker cannot rely on being able to prompt for permission. It reports
`permission-required`, and main JavaScript performs any user-activated
permission flow.

### Source behavior

Filesystem handles are revocable capabilities, not durable storage:

- Permission may be lost.
- A selected file may move, change, or disappear.
- A `File` represents one observed source state.
- Portable file watching is not assumed.
- Reload and directory rescan are explicit operations.
- No absolute path is exposed or used as a resource identity.

Directory-relative glTF dependencies are resolved one safe segment at a time.
Absolute references and `..` traversal outside the granted directory are
rejected.

### Payload transport

Every load is tagged with a `load_id` and source revision. Payload delivery is
bounded, backpressured, and cancellable.

If the importer requires one contiguous GLB, the initial implementation
transfers one complete `ArrayBuffer`. Chunked transport is added only with an
incremental parser; otherwise chunking adds protocol complexity without
reducing peak decode memory.

The I/O worker never mutates canonical `RenderData`.

### Geometry release policy

Source-backed CPU geometry may be released only when it is reconstructible
from a currently known source capability or is no longer needed. GPU-resident
geometry may remain renderable without a CPU copy.

If reload later fails because access was revoked or the source disappeared,
the geometry handle remains valid but transitions to
`source_unavailable/nonresident`.

Procedural or unsaved geometry with no recoverable source is pinned. When its
budget is exhausted, new allocations are rejected rather than silently
evicting the only copy.

## Pure import and transactional commit

Asset decoding does not create GPU resources. A glTF importer returns a pure
owned CPU result:

```text
filesystem payload
  → decoder
  → ImportedScene
      ├── local-space geometry
      ├── node instances and transforms
      ├── pipeline/material requests
      └── scene bounds metadata
  → frame-boundary validation
  → atomic scene commit
  → GPU allocation and upload
```

No renderer borrow survives an asynchronous operation. Failed or cancelled
imports leave previously committed scene state unchanged.

## GPU mirror

The GPU is a mirror of canonical `RenderData`, not the owner of resource
identity.

```rust
pub struct GpuMirror {
    positions: GpuSlab,
    normals: GpuSlab,
    uvs: GpuSlab,
    indices: GpuSlab,
    instances: GpuInstanceBuffer,
    culling: GpuCullingData,
    geometry_allocations: Vec<Option<GpuGeometryAllocation>>,
}
```

Separate position, normal, and UV buffers retain the current shader layout,
but all three use one logical vertex-range allocator. Indices remain local to
the geometry and draws use `base_vertex = vertex_start`.

```rust
pub struct GpuGeometryAllocation {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}
```

### GPU growth

`wgpu::Buffer` objects cannot resize. To double a slab:

1. Validate the new capacity against budgets and device limits.
2. Create the larger buffer.
3. Copy the old prefix to identical offsets.
4. Upload new and dirty ranges.
5. Switch the mirror before constructing the next render queue.
6. Retain the old buffer until submitted work using it has completed.

Ordinary growth preserves all existing offsets. Compaction and relocation are
deferred.

GPU allocation, growth, uploads, and resource replacement are batched before
render queue construction at a frame commit boundary.

## Ready set and resolved render queue

Application-level resource validation occurs when dependencies or instance
state change, not inside the draw loop.

The render worker maintains reverse dependency indices:

```text
GeometryHandle → dependent instances
PipelineKey    → dependent instances
MaterialKey    → dependent instances
```

Geometry residency and pipeline readiness transitions dirty only affected
instances.

```rust
pub struct ResolvedDrawState {
    pub pipeline_bucket: u32,
    pub gpu_slab: u32,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub index_format: IndexFormat,
    pub base_vertex: i32,
    pub instance_slot: u32,
    pub material_binding: u32,
}
```

Only fully resolved instances enter the dense `ReadySet`. Render queue packets
contain all execution state required by a draw and are partitioned by complete
pipeline/binding compatibility.

```rust
pub struct RenderQueueHeader {
    pub scene_commit_epoch: u32,
    pub gpu_resource_epoch: u32,
    pub pipeline_epoch: u32,
    pub spatial_snapshot_id: u32,
}
```

The renderer checks these epochs once at draw entry. It performs no per-item
generational, residency, pipeline-readiness, or geometry-to-buffer lookup.

GPU resources referenced by a queue remain frozen and alive through submission
completion.

## Local AABB worker

Local bounds belong to geometry and are maintained in a separate shared SoA.

```rust
pub struct LocalBoundsData {
    pub generations: Vec<u32>,
    pub content_versions: Vec<u32>,
    pub states: Vec<BoundsState>,
    pub minimums: Vec<[f32; 3]>,
    pub maximums: Vec<[f32; 3]>,
}

pub enum BoundsState {
    Pending,
    Valid,
    Empty,
    InvalidNonFinite,
}
```

### Bounds jobs

Each job includes:

```text
geometry slot and generation
geometry content version
geometry snapshot ID
position format and count
immutable page/span descriptors
geometry layout epoch
```

The bounds worker receives a sealed immutable geometry snapshot. Referenced
pages are leased and cannot be overwritten, relocated, or released until the
worker acknowledges release.

Each result is keyed by the same generation, content version, and snapshot ID.
The render worker accepts only an exact match; stale output is discarded.

Immutable geometry is scanned once in full. If mutable paged geometry is later
supported, each dirty page is fully rescanned and all current page bounds are
reduced. Scanning only changed vertices is insufficient because it cannot
detect an old extremum moving inward.

Pending, empty, or invalid bounds never produce false-negative culling. Their
instances use conservative visibility until valid bounds are accepted.

## Spatial snapshots and scene BVH worker

The scene BVH is for broad-phase picking and other CPU-side spatial queries. It
is not the authoritative render-culling path.

The render worker publishes one coherent immutable `SpatialSnapshot` rather
than allowing the BVH worker to independently sample changing SoA columns.

```rust
pub struct SpatialSnapshotHeader {
    pub snapshot_id: u32,
    pub scene_commit_epoch: u32,
    pub instance_count: u32,
}
```

Each leaf contains:

- Mesh slot and generation.
- Geometry slot and generation.
- Accepted local-bounds identity.
- Transform and transform version.
- Visibility, selectability, and layer masks.
- Leaf membership metadata.

Only accepted local bounds enter the snapshot. The BVH worker derives world
AABBs and builds or refits the tree.

### Refit and rebuild policy

- Refit when only transforms or local bounds changed and topology/membership
  remains stable.
- Rebuild after additions, removals, slot reuse, representation changes, or
  spatial membership changes.
- Rebuild when a quality metric shows that repeated refits have degraded the
  tree.

### Publication and leases

The BVH worker writes an inactive immutable buffer and atomically publishes:

```text
buffer index
publication version
spatial snapshot ID
node count
leaf count
```

Published buffers are never overwritten while leased by rendering diagnostics
or picking readers. Two buffers are used initially; the worker coalesces or
waits when the inactive buffer is still leased. A third buffer is added only if
profiling shows that backpressure is significant.

The renderer never waits for a BVH. BVH latency affects spatial-query
availability but not frame rendering.

## Picking contract

Each presented frame publishes:

```text
frame ID
scene commit epoch
spatial snapshot ID
camera/projection epoch
viewport
```

A pick request identifies the presented frame used to construct its ray. The
BVH worker queries the matching immutable spatial snapshot and returns
candidate mesh handles.

If the matching snapshot has been retired, the operation returns
`stale/retry`; it does not silently query current state. Returned generational
handles are revalidated before an action affects current canonical state.

The scene AABB BVH is broad phase only. Exact picking later uses either:

- CPU triangle intersection against matching geometry/transform snapshots.
- A GPU object-ID and depth picking pass.

## GPU compute culling

The compute shader is authoritative for render visibility. It uses accepted
local AABBs and current world matrices and does not depend on the scene BVH.

```rust
pub struct GpuCullingData {
    pub local_bounds: wgpu::Buffer,
    pub world_matrices: wgpu::Buffer,
    pub candidate_instances: wgpu::Buffer,
    pub visible_instances: wgpu::Buffer,
    pub indirect_arguments: wgpu::Buffer,
    pub bucket_counters: wgpu::Buffer,
    pub culling_uniforms: wgpu::Buffer,
}
```

Only fully resolved `ReadySet` members become culling candidates. Candidate
records contain resolved indices into local bounds, matrices, indirect
arguments, and pipeline/geometry buckets. The compute shader performs
visibility testing, not handle or resource validation.

### Local-to-world AABB conversion

For an affine world transform, the shader computes a conservative world AABB:

```text
local_center = (local_min + local_max) × 0.5
local_extent = (local_max - local_min) × 0.5

world_center = world_matrix × vec4(local_center, 1)
world_extent = abs(mat3(world_matrix)) × local_extent

world_min = world_center - world_extent
world_max = world_center + world_extent
```

This supports translation, rotation, non-uniform scale, reflection, and affine
shear conservatively without transforming all eight corners.

Instances with pending or invalid bounds use an explicit `ALWAYS_VISIBLE` flag
rather than a fabricated zero or infinite AABB.

### Frustum culling

The compute pass receives current camera/frustum data and:

1. Resets output counters and visibility fields.
2. Derives conservative world AABBs.
3. Tests candidates against frustum planes.
4. Writes visible instance indices or per-packet visibility.
5. Writes indirect draw arguments.

Local bounds, world matrices, and candidate records used by one dispatch belong
to the same committed scene epoch.

### Indirect rendering

The initial implementation prebuilds one indirect argument record per resolved
draw packet. Compute writes `instance_count = 0` for culled packets and `1` for
visible singular instances. The CPU loops over dense packets and calls
`draw_indexed_indirect` without resource or visibility branches.

Later, compatible instances are grouped by pipeline, material, and geometry.
Compute compacts visible instance indices into bucket ranges and writes one
indirect argument record per bucket. Multi-draw indirect may be used where
supported, but it is not required by the baseline architecture.

No GPU visibility readback occurs in the frame path.

### Future occlusion culling

The same compute stage later consumes a hierarchical Z buffer:

1. Retain or render depth.
2. Build an HZB pyramid.
3. Project each frustum-visible world AABB.
4. Choose a mip level from projected size.
5. Perform a conservative depth test.
6. Update indirect visibility or compacted bucket output.

The initial occlusion path uses the previous completed frame's HZB. Camera
cuts, major projection changes, newly created instances, and uncertain bounds
bypass occlusion for at least one frame. Ambiguous tests remain visible.

## Unified frame lifecycle

### Parallel producer work

Between render commits:

1. Main JavaScript accumulates and coalesces its mutation journal.
2. The filesystem worker reads requested source files.
3. The bounds worker computes local AABBs from sealed geometry snapshots.
4. The BVH worker builds or refits an unpublished scene tree.

### Render-worker commit boundary

The render worker:

1. Acquires at most one complete credited command batch.
2. Snapshots completed I/O, bounds, BVH, pipeline, and device events; later
   arrivals wait for the next commit.
3. Validates the batch envelope and applies semantic commands in journal order.
4. Commits completed imports transactionally.
5. Advances canonical generations and content/state versions.
6. Accepts exact-match local bounds results and discards stale results.
7. Applies pipeline state transitions, GPU allocation/growth, uploads, and
   residency changes.
8. Updates reverse dependencies, the `ReadySet`, and `ResolvedDrawState` only
   for dirty instances and resources.
9. Dispatches bounds jobs for geometry content versions lacking accepted
   bounds.
10. Publishes a coherent `SpatialSnapshot` and dispatches or coalesces its BVH
    job.
11. Builds or patches resolved GPU culling candidates and indirect records.
12. Seals the candidate render queue and freezes referenced resources.
13. Publishes the committed JavaScript projection.
14. Publishes command and import completions.

### Compute and render

The renderer then:

1. Checks the render queue epochs once.
2. Uploads current camera/frustum culling uniforms.
3. Encodes the culling compute pass.
4. Relies on WebGPU pass ordering and buffer usages for compute-to-indirect
   synchronization.
5. Encodes pipeline buckets and indirect draw packets without per-item
   application validation.
6. Submits and presents.
7. Publishes the presented-frame record used by picking.
8. Releases geometry-snapshot and BVH leases no longer needed by the cycle.
9. Retires replaced GPU resources after submission completion permits it.
10. Acknowledges the consumed mutation batch and grants the next single credit,
    even if presentation was skipped or failed.

## Failure and fallback behavior

- A stale JS handle rejects at command commit.
- A full command queue preserves the entire pending journal and applies
  backpressure; it never drops commands.
- A failed import leaves the old committed scene unchanged.
- Missing filesystem permission returns `permission-required`.
- Missing or changed source files produce explicit source states.
- Missing local bounds force conservative visibility.
- A stale or unavailable BVH makes picking retry or wait; it never affects
  render visibility.
- A GPU culling uncertainty keeps the candidate visible.
- Pipeline or geometry resources that are not GPU-ready remain outside the
  ready set.
- Device or surface loss is handled outside the draw-packet loop.
- Budget exhaustion returns an explicit allocation error rather than silently
  exceeding the configured cap.

## Implementation phases

### Phase 0: Shared contracts and worker routing

**Status: implemented.** The browser and render worker share one versioned ABI
root, framed command/completion rings, coherent projection epochs, and a single
frame credit. The persistent worker dispatcher routes I/O, bounds, BVH, and
picking traffic without replacing canvas or protocol listeners.

- Define handles, ABI root, command records, completion records, epochs, and
  state enums in one versioned contract.
- Replace startup `onmessage` replacement with a persistent worker dispatcher.
- Add startup checks for shared memory, atomics, cross-origin isolation, ABI
  compatibility, and required browser APIs.
- Implement the frame-credit and complete-batch publication protocol.
- Add pure tests for command-ring wraparound, full conditions, malformed
  batches, and stale tokens.

### Phase 1: Canonical CPU RenderData

**Status: implemented for transactional scene replacement.** Canonical
`RenderData` owns generational geometry/instance slot tables, global geometry
SoA arrays with checked geometric growth, and fixed-slot instance SoA columns.
Imports are built independently and committed only after CPU and GPU preflight;
the demo's `loadScene` operation intentionally replaces the displayed scene
rather than accumulating every selected file.

- Add generational geometry and instance slot tables.
- Add the fixed-slot instance SoA with configurable doubling.
- Add paged/chunked geometry SoA storage and checked budgets.
- Refactor glTF import into a pure CPU `ImportedScene`.
- Preserve primitive-local geometry and create node instances separately.
- Make scene replacement transactional; expose additive creation through
  `Mesh.clone()` without changing geometry ownership.
- Temporarily adapt the existing per-geometry GPU resources to consume
  canonical `RenderData`.
- Test stale handles, slot reuse, shared glTF primitives, failed growth, and
  failed imports.

### Phase 2: GPU mirror and resolved render queue

**Status: implemented.** Global geometry, model, bounds, culling, and indirect
buffers use persistent slabs with checked doubling and GPU prefix copies.
Immutable geometry is not recreated for instance-only changes. Draw validity
and pipeline selection are resolved at frame commits, leaving the indirect draw
loop free of handle, geometry, and pipeline validation.

- Add large position, normal, UV, index, and instance buffers.
- Add stable-offset range allocators.
- Implement checked buffer doubling and old-prefix copies.
- Batch dirty uploads at frame boundaries.
- Add geometry/pipeline reverse dependency indices.
- Add `ReadySet`, `ResolvedDrawState`, and pipeline-bucketed render queues.
- Remove per-mesh GPU buffers and `MeshBuilder` after the mirror path works.
- Verify geometry remains drawable through multiple buffer growth events.

### Phase 3: JavaScript Mesh API and render-cycle gate

**Status: implemented.** The initial bounded projection and rings use the
existing shared Wasm memory. Filesystem workers, worker-driven bounds/BVH, and
GPU culling remain later phases and are not implied by this status.

- Publish the stable ABI root and JS read projection.
- Add thin generational `Mesh` wrappers.
- Implement the ordered mutation journal and coalescing segments.
- Add lifecycle barriers and immediate JS tombstoning.
- Add the SPSC command ring, completion ring, and one-credit protocol.
- Route every external mesh mutation through the Mesh API.
- Stress retained JS views across projection and Wasm memory growth.

### Phase 4: Filesystem worker and transactional loading

**Status: implemented for the current GLB scope.** A dedicated module worker
reads bundled URLs and structured-cloned file handles, then transfers one
contiguous GLB buffer directly to the render worker over a bounded,
acknowledged `MessageChannel`. Load IDs rendezvous payload registration with a
frame-gated lifecycle command; superseded loads are discarded and failures
retain the existing scene. Main-thread picker integration includes a `File`
fallback. Directory/external glTF dependencies are explicitly unsupported in
this phase, avoiding unsafe relative-path resolution.

- Add main-thread file and directory picker integration.
- Structured-clone handles to a dedicated I/O worker.
- Add a direct I/O/render-worker `MessageChannel`.
- Implement permission, cancellation, source revision, and read-failure states.
- Add bounded payload transfer and backpressure.
- Resolve relative glTF dependencies safely through directory handles.
- Keep the existing scene valid across every loading failure mode.

### Phase 5: Local bounds worker

**Status: implemented.** A persistent, single-writer bounds worker receives
transferred immutable position-copy snapshots and publishes exact-identity
results through a versioned, fixed-capacity (1,048,576 maximum slots) Wasm
`SharedArrayBuffer` SoA using per-slot seqlocks. Capacity is configured from
the geometry projection (minimum 1024); overflow is explicitly logged and
retains conservative pending visibility rather than omitting geometry. The
render worker accepts results only at frame commit boundaries and mirrors
accepted bounds/state into a GPU storage buffer. CPU bounds retained by
`Geometry::new` are provisional and used only for camera framing. Dynamic SAB
descriptor replacement is deferred because the fixed maximum matches the
current geometry-store maximum.

- Add shared `LocalBoundsData` and versioned result mailboxes.
- Add immutable geometry snapshot/page leases.
- Compute full local AABBs for immutable geometry.
- Reject stale generation/content-version results.
- Upload accepted bounds to the GPU culling buffer.
- Verify pending/invalid bounds remain conservatively visible.

### Phase 6: GPU frustum culling and indirect draws

**Status: implemented.** Singular resolved packets are prebuilt as WebGPU
indirect records. A compute pass conservatively clip-tests transformed AABBs
against the WebGPU/DX clip volume and writes only `instance_count`; rendering
uses global geometry/model buffers and has no visibility readback or per-draw
application validity branch. Pending/empty/invalid/missing bounds and
`ALWAYS_VISIBLE` fail open. Shared-instance compaction remains a later
optimization, as allowed by this phase's singular-instance baseline.

- Add GPU culling inputs, outputs, uniforms, and indirect argument buffers.
- Implement local-to-world AABB conversion in WGSL.
- Implement conservative frustum tests.
- Write `instance_count` or compacted visible-instance outputs.
- Replace direct draws with resolved indirect draw packets.
- Verify no visibility readback or per-item application validation occurs.
- Add shared-geometry instance compaction after singular instances work.

### Phase 7: Spatial snapshots, BVH, and picking

**Status: implemented.** The render worker publishes owned structured-clone
snapshots rather than exposing canonical columns. The dedicated BVH worker
median-rebuilds on membership changes and refits stable topology. BVH results
use immutable message publications with an explicit publication-generation
lease/ack (a deliberate simpler deviation from SAB double buffering). Picking
is presented-frame stamped broad-phase AABB picking only; stale snapshots are
rejected and returned generational handles are revalidated before selection is
reported. BVH latency never gates rendering.

- Add coherent immutable `SpatialSnapshot` publication.
- Add the dedicated BVH worker.
- Implement world-AABB derivation, rebuild, and refit policies.
- Add immutable BVH buffer publication and reader leases.
- Add presented-frame-aware broad-phase picking.
- Add stale/retry behavior and handle revalidation.
- Add exact narrow-phase picking separately when required.

### Phase 8: HZB occlusion culling

**Status: implemented.** Two `R32Float` pyramids are ping-ponged. After the
depth render, compute copies sampled `Depth32Float` into mip zero and max-
reduces every level (including odd-edge texels). Frame N culling samples only
the completed frame N-1 pyramid. Projected world AABBs fail open at the near
plane, screen edge, invalid projection, pending bounds, and during first-frame,
resize, scene-change, or camera-cut grace. Conventional 0-near/1-far depth
requires max reduction: an AABB is hidden only when its nearest depth is
strictly behind the farthest depth over every covered HZB texel plus bias.
Controls are exposed through `Renderer::occlusion_config`.

- Build a hierarchical depth pyramid.
- Add conservative projected-AABB occlusion tests.
- Use previous-frame depth initially.
- Add camera-cut and newly-visible bypass policies.
- Profile false-positive visibility, dispatch cost, and bucket compaction before
  introducing more complex GPU-driven rendering features.

## Required invariants

1. The render worker is the sole writer of canonical `RenderData`.
2. Every shared region has one designated writer.
3. Auxiliary output becomes accepted state only at a render-worker commit
   boundary.
4. Every external resource reference is a typed slot and generation.
5. Array growth never changes a live logical handle.
6. Destruction and slot reuse invalidate every old handle.
7. Async callbacks enqueue owned results and never mutate canonical or GPU
   state directly.
8. One credit authorizes at most one complete command batch.
9. Queue overflow never drops or partially publishes a command batch.
10. Coalescing applies only to absolute assignments and never crosses a
    lifecycle/resource barrier.
11. Commands are applied in journal order against state produced by earlier
    commands in the same batch.
12. Projection readers observe either complete pre-commit or complete
    post-commit state.
13. Projection publication precedes completion publication.
14. Geometry pages referenced by a bounds job remain immutable and alive until
    lease release.
15. A local AABB is accepted only for an exact geometry generation, content
    version, and snapshot identity.
16. Missing, stale, empty, or invalid bounds never cause false-negative
    culling.
17. A `SpatialSnapshot` is minted only from one coherent committed state.
18. A BVH buffer and all its leaves belong to one immutable spatial snapshot.
19. Published BVH buffers are never overwritten while leased.
20. BVH refit preserves leaf topology; structural changes rebuild.
21. BVH availability does not gate rendering.
22. Picking identifies a presented frame and matching spatial snapshot.
23. BVH picking is broad phase until a matching narrow phase is implemented.
24. Ready-set membership means every required pipeline, binding, and GPU
    allocation is resolved.
25. A render queue is valid only for its stamped scene, resource, and pipeline
    epochs.
26. GPU resources referenced by a queue remain immutable and alive through
    submission completion.
27. The draw loop performs no per-item handle, geometry-residency,
    pipeline-readiness, or visibility validation.
28. GPU culling inputs belong to one committed scene epoch.
29. Pending or uncertain GPU culling inputs force visibility.
30. GPU visibility reaches rendering through indirect output without CPU
    readback.
31. Occlusion culling removes work only when occlusion is proven.
32. Filesystem handles are treated as revocable capabilities.
33. The filesystem worker never mutates canonical state.
34. I/O payloads are bounded, backpressured, tagged, and cancellable.
35. Failed imports and reloads leave existing committed scene state intact.
36. Geometry without a recoverable source is never silently evicted.
37. Capacity and budget accounting includes reserved capacity and known
    transient copies, not only live element counts.
38. Successful raw geometry creation creates exactly one instance by default.

## Deferred extensions

The architecture intentionally leaves room for, but does not initially include:

- GPU indirect draw compaction across all compatible instances.
- Multi-draw indirect where device support permits it.
- Compute-driven LOD selection.
- HZB occlusion beyond the conservative initial policy.
- GPU picking and exact CPU triangle picking.
- CPU and GPU arena compaction.
- Copy-on-write mutable geometry.
- Materials, textures, animation, and skinning SoAs.
- Render bundles and pass-specific ready sets.
- Separate simulation and physics spatial structures.
- File-change observation where browser APIs support it.

Each extension must preserve the ownership, generation, versioning, commit,
and conservative-fallback invariants defined above.
