# glTF import worker

`@yawn/gltf-import` keeps parsing and bulk upload off the renderer command channel. It fetches a `.gltf` or `.glb` URL in a dedicated worker and writes a format-neutral packet directly into shared memory.

## Load a scene

```js
import { GltfImporter } from "@yawn/gltf-import";

const importer = new GltfImporter(core);
try {
  const result = await importer.load("/models/level.glb");
  console.log(result.meshes, result.materials, result.bounds);
} finally {
  importer.dispose();
}
```

The import handshake is:

1. The import worker fetches and measures the asset packet.
2. Core allocates a fixed `upload.renderData` shared array.
3. The import worker writes packet bytes into that SAB.
4. Core receives only the array ID and byte count, then installs render data.

No GLB payload is copied through the renderer's message queue.

<Playground
  id="gltf-worker"
  title="Worker-side glTF import"
  description="A generated GLB is fetched through an object URL and committed from shared upload memory."
/>

## Camera framing

Import frames the canonical `camera.state` row from scene bounds by default. Select an exterior or interior framing policy, or preserve the current camera.

```js
await importer.load(url, { framing: "exterior" });
await importer.load(url, { framing: "interior" });
await importer.load(url, { framing: false });
```

Framing is an addon behavior implemented as a shared camera-row write. It is not a camera subsystem in core.

## Wrap the result when useful

Import returns protocol descriptors. Less technical consumers can turn those descriptors into generation-safe objects.

```js
import { MeshHandles, MaterialHandles } from "@yawn/mesh-handles";

const meshes = new MeshHandles(core).fromImportedScene(result);
const materials = new MaterialHandles(core).fromImportedScene(result);
```
