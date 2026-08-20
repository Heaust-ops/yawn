# glTF import and picking

## Import off-thread

The shared importer worker fetches and parses glTF/GLB, then the response hydrates `Mesh` and `PBRMaterial` handles attached to your scene.

```ts
import { importGltf } from "@yawn/handles";

const meshes = await importGltf(scene, "/models/sponza.glb");
meshes[0].position[1] = 0.5;
```

The importer handles triangle primitives, external/data buffers, standard vertex attributes, indices, node transforms, and metallic-roughness values. Application-specific extensions remain application policy.

## Pick in the BVH worker

```ts
const picking = new Picking(scene);
await picking.ready;

canvas.addEventListener("click", async () => {
  const hits = await picking.pick([0, 0, 4], [0, 0, -1]);
  const nearest = hits[0];
  if (nearest) console.log(nearest.id, nearest.distance);
});
```

The worker reads shared node positions and mesh bounds, updates its BVH when the shared frame counter changes, and returns **all** AABB hits sorted by distance. You can shortlist or run an exact test afterward.

If row allocations relocated since `Picking` was created, refresh its shared descriptors:

```ts
await picking.refresh();
```

The playground below imports the repository's full LFS-backed `sponza.glb` in the importer worker, hydrates all 138 primitives, frames them with an arc camera, and sends a real ray to the BVH worker. Click the preview to pick again.

<Playground example="importing" />

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
