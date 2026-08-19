# Runtime interaction recipes

Once render data exists, keep hot updates on shared rows and leave core free of application policy.

## 12 — Animate directly through the SAB

The instance facade performs the live-generation check and writes `instance.transform`.

```js
function frame(time) {
  instance.setTransform(rotationY(time * 0.001));
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
```

<Playground id="shared-animation" title="Frame-rate transform writes" />

## 13 — Pick through the optional BVH worker

```js
const result = await meshHandles.pickRay(origin, direction, {
  maxDistance: 10_000,
  maxHits: 1,
});
const nearest = result.hits[0]?.instance;
```

The picking addon consumes versioned shared snapshots. Core does not know about rays or BVHs.

<Playground id="picking" title="Pick a shared instance" />

## 14 — Connect worker to worker

`MessagePort` implements the Worker-like methods the core client needs. Start it through the normal `YawnCore` constructor.

```js
import { YawnCore } from "@yawn/core";

const core = new YawnCore({
  worker: port,
  memory,
  ringPtr,
  free: () => port.close(),
});
await core.ready;
```

This is why “main thread” is not an architectural role in Yawn: any browser worker can own the client.

## 15 — Compose a complete scene

Use one core instance for every addon and activate the graph only after its complete loadout has compiled.

```js
const importer = new GltfImporter(core);
const imported = await importer.load(gltfUrl);
const meshes = new MeshHandles(core).fromImportedScene(imported);
const compiled = await loadGraph(core, completeGraph);
await core.switchCompiledGraph(compiled.compiledId);
```

<Playground id="first-scene" title="Complete addon composition" />

## 16 — Treat camera input as render data

There is no camera API in core. Read and write the canonical 16-lane row directly from controls or simulation code.

```js
const camera = core.array("camera.state");
const state = camera.read(0);
state.splice(0, 3, ...nextEye);
camera.write(0, state);
```

The row packs eye, target, up, field of view, aspect, near, and far values into 64 bytes.

## 17 — Use conventional camera and material properties

Choose addon handles when a property-oriented workflow is more useful than raw SOA rows.

```js
const camera = new CameraHandle(core);
const materials = new MaterialHandles(core).fromImportedScene(imported);

camera.lookAt([4, 3, 6], [0, 0, 0]);
materials[0].baseColor = [0.2, 0.55, 1, 1];
materials[0].roughness = 0.35;
```

<Playground id="conventional-handles" title="Camera and material handles" />
