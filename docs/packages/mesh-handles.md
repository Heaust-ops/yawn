# Conventional handles

`@yawn/mesh-handles` is an optional object-oriented facade. It never hides core: lifecycle methods call core commands, while frequent mutations write shared rows.

## Meshes and instances

Wrap imported descriptors, use the default instance created by glTF, or create another generation-safe instance.

```js
const handles = new MeshHandles(core);
const [mesh] = handles.fromImportedScene(imported);

mesh.defaultInstance.setTransform(matrix);
const duplicate = await mesh.createInstance(otherMatrix);
duplicate.setType(classificationWords);
await duplicate.destroy();
```

The handle is `[slot, generation]`. A stale object cannot modify a slot that has since been reused.

## Camera and material handles

The camera and materials look conventional, but property updates are writes to `camera.state` and `material.state`.

```js
const camera = new CameraHandle(core);
camera.lookAt([4, 3, 6], [0, 0, 0]);

const materials = new MaterialHandles(core).fromImportedScene(imported);
materials[0].baseColor = [0.2, 0.55, 1, 1];
materials[0].roughness = 0.35;
```

<Playground
  id="conventional-handles"
  title="Camera and material properties"
  description="The gallery is reframed and one PBR row is restyled with direct shared-memory writes."
/>

## Worker-side picking

Picking is lazy. The first `pickRay` starts a separate spatial-query worker over versioned render-data snapshots and returns wrapped instance handles.

```js
const result = await handles.pickRay(origin, direction, {
  maxDistance: 10_000,
  maxHits: 1,
});

const picked = result.hits[0]?.instance;
```

<Playground
  id="picking"
  title="Pick the closest shared instance"
  description="Build the optional BVH and issue a ray query without adding picking code to core."
/>

Call `handles.dispose()` when the scene ends so its optional picking worker and listeners are released.
