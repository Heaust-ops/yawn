# Yawn

Yawn is a Rust/WGPU renderer whose application boundary is worker messages plus
shared WebAssembly memory. Backward compatibility is intentionally deferred until
1.0.

## Architecture

```text
FXNode ───────────────┐
                     ├─> canonical DAG AST ─> S-expression ─> Yawn render worker
JavaScript objects ──┘                                      │
                                                            ├─> graph compiler
                                                            ├─> transient allocator
                                                            └─> prepared GPU loadout

Any browser thread ── infrequent commands ──────────────────> worker
Any browser thread ── atomic SOA writes ────────────────────> shared WASM memory
glTF import worker ── fetch URL ──> fixed shared SOA upload ─┘
```

The canonical AST is the only public render-graph wire format. Nodes are named
definitions and `(ref "node" "socket")` forms are edges, so an output can fan out
without expanding into a tree. Core parses the S-expression, validates the DAG,
culls dead work, calculates resource lifetimes, aliases compatible non-overlapping
transients, coalesces render passes, and allocates the resulting textures and GPU
pipelines before activating a graph.

Authored render shaders use Yawn's fixed scene ABI. Render and compute declarations
carry source, entry points, and dispatch/state metadata and are prepared with the
graph loadout. Core contains no built-in shader source or pipeline declarations.

## Packages

- `packages/yawn-core` (`@yawn/core`) — the worker command transport, serialized
  graph lifecycle, and shared SOA views; it returns `[slot, generation]` handles.
- `addons/render-graph-ast` — canonical immutable DAG AST and S-expression serializer.
- `addons/render-graph-js` — plain-object/fluent graph APIs that serialize and load ASTs.
- `addons/render-graph-fxnode` — FXNode snapshot exporter and diagnostic mapping.
- `addons/default-pipelines` — optional scene/frame shader and compute declarations.
- `addons/gltf-import` — URL-fetching worker that writes GLB bytes directly to a fixed SOA.
- `addons/mesh-handles` — conventional `Mesh`/`Instance` objects and optional BVH picking.

The integration example in `examples/render-graph-studio` consumes every package
through its public API; no example source or shader lives in core.

The focused recipes in `examples/cookbook` show each addon independently, including
AST/JSO/FXNode authoring, external render and compute programs, graph activation,
shared glTF import, mesh instances, custom SOA columns, SAB animation, picking, and
worker-to-worker use.

Example graph authoring:

```js
import { RenderGraph, ref } from "@yawn/render-graph-js";
import { defaultPipelines } from "@yawn/default-pipelines";

const graph = new RenderGraph("main", 1)
  .renderPipeline(defaultPipelines.render[1])
  .renderPipeline(defaultPipelines.render[2])
  .renderPipeline(defaultPipelines.render[3])
  .computePipeline({
    name: "prepare",
    shader: "@compute @workgroup_size(1) fn main() {}",
    entry: "main",
    dispatch: [1, 1, 1],
  })
  .node("mesh", "mesh", { version: 2 })
  .node("draw", "gltf_standard", {
    version: 2,
    inputs: { mesh: [ref("mesh", "mesh")] },
  });

// Add the required attachments and frame output, then let the addon own the wire encoding:
await graph.load(core);
```

## Shared render data

`@yawn/core` exposes 64-byte-aligned shared SOA columns. Every stride is a multiple
of 16 bytes and scalar lanes are atomic `u32`, `i32`, or IEEE-754 `f32` bits. The
built-in instance transform/type columns are generation-guarded so a stale handle
cannot mutate a reused slot.

Allocate application columns infrequently through the worker:

```js
const velocity = await core.allocateArray({
  name: "instance.velocity",
  domain: "instance", // also "mesh" or "fixed"
  scalar: "f32",
  lanes: 4,
});

velocity.write(instanceSlot, [1, 0, 0, 0]);
```

Import a GLB without transferring its bytes through renderer messages:

```js
import { GltfImporter } from "@yawn/gltf-import";
import { MeshHandles } from "@yawn/mesh-handles";

const importer = new GltfImporter(core);
const handles = new MeshHandles(core);
const meshes = handles.fromImportedScene(await importer.load(gltfUrl));
meshes[0].defaultInstance.setTransform(nextTransform); // direct shared-SOA write
```

The renderer grows mesh/instance-domain columns with render-data capacity and
publishes replacement descriptors through the core's `yawn-soa-layout` event.
Typed-array views refresh when shared WASM memory grows. Messages are reserved for
allocation and lifecycle operations. Existing instance values and bulk asset uploads
use shared memory; a GLB commit message contains only an array ID and byte count.

`YawnCore` accepts a transport bridge whose worker endpoint can be a `Worker` or a
started `MessagePort`, so the same API can run on the browser main thread or another
worker. Optional picking is installed with the mesh addon's `createPickingWorker`.

Cross-origin isolation is required (`COOP: same-origin`, `COEP: require-corp`). The
Vite development and preview servers already set both headers.

## Development

```sh
npm run dev
npm run test:js
cargo check --workspace
```

Production build:

```sh
npm run build-release
```
