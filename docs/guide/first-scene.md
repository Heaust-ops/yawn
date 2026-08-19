# Your first scene

This tutorial composes Yawn the same way an application does: create the worker transport, wait for shared render data, compile a graph, import an asset, and activate the prepared loadout.

## 1. Start core

Create one renderer worker and transfer an `OffscreenCanvas` to it. `YawnCore` is deliberately transport-oriented; your bootstrap owns canvas sizing and worker construction.

```js
import { YawnCore } from "@yawn/core";

const canvas = document.querySelector("canvas");
canvas.width = Math.round(canvas.clientWidth * devicePixelRatio);
canvas.height = Math.round(canvas.clientHeight * devicePixelRatio);
const offscreen = canvas.transferControlToOffscreen();
const worker = new Worker(new URL("./render-worker.js", import.meta.url), {
  type: "module",
});
const core = new YawnCore({ worker });
worker.postMessage({ type: "init", canvas: offscreen }, [offscreen]);
await core.ready;
```

`ready` resolves after core receives the standard SOA descriptors. From that point, `core.array("camera.state")` and the other built-in columns are safe to access.

## 2. Compile a graph

The optional default-pipelines addon supplies scene WGSL. A JSO graph places those declarations beside graph nodes, then the graph addon serializes the canonical AST for core.

```js
import { loadGraph } from "@yawn/render-graph-js";
import { defaultPipelines } from "@yawn/default-pipelines";

const graph = {
  id: "main",
  revision: 1,
  pipelines: defaultPipelines,
  nodes: completeSceneNodes,
};

const compiled = await loadGraph(core, graph);
```

Compilation validates the DAG, removes dead work, computes transient resource lifetimes, aliases compatible resources, and allocates the resulting loadout before returning its ID.

## 3. Import render data

The glTF addon fetches and parses in its own worker. It asks core for a fixed shared upload array, writes the packet into that SAB, then sends only the array ID and byte count for the commit.

```js
import { GltfImporter } from "@yawn/gltf-import";
import { MeshHandles } from "@yawn/mesh-handles";

const importer = new GltfImporter(core);
const result = await importer.load("/assets/scene.glb");
const handles = new MeshHandles(core);
const meshes = handles.fromImportedScene(result);
importer.dispose();
```

## 4. Activate the loadout

Switching is transactional from the application's perspective: the previously active loadout keeps rendering until the prepared graph becomes active.

```js
await core.switchCompiledGraph(compiled.compiledId);

meshes[0].defaultInstance.setTransform(nextTransform); // direct SAB write
```

Use messages for setup and teardown. Use shared writes for values that are already present and can change every frame.

<Playground
  id="first-scene"
  title="Complete first scene"
  description="Open the editor to change the procedural loadout or inspect live telemetry."
/>

## Next steps

- Learn why these boundaries exist in [How Yawn fits together](./architecture).
- Author graphs with [plain objects, a fluent builder, or FXNode](../packages/render-graph).
- Add custom shared columns in [Core and render data](../packages/core).
- Use familiar objects in [Conventional handles](../packages/mesh-handles).
