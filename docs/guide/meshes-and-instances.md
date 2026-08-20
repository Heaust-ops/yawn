# Meshes and instances

Every `Mesh` is an instance. `clone()` shares geometry and allocates only a new node/mesh slot.

```ts
const source = new Mesh(scene, {
  vertexData: {
    positions: [-0.2, -0.2, 0, 0.2, -0.2, 0, 0, 0.25, 0],
    indices: [0, 1, 2],
  },
});
await source.ready;

for (let x = -4; x <= 4; x++) {
  const instance = source.clone({ position: [x * 0.2, 0, 0] });
  await instance.ready;
}
```

<Playground example="instances" />

## Copy-on-write geometry

The default vertex kinds are `positions`, `normals`, `tangents`, `uvs`, `colors`, and `indices`. Mutating any kind on an instanced clone first makes that mesh's geometry unique.

```ts
const clone = source.clone();
await clone.ready;

await clone.setVertexData("positions", [
  -0.4, -0.2, 0,
   0.4, -0.2, 0,
   0.0,  0.5, 0,
]);
```

## Per-face materials and visibility

```ts
await mesh.setMaterialForFaces(red, [0, 2, 4]);
mesh.material = blue;
mesh.isVisible = false; // one direct u32 write
```

Face rows store optional material pointers; a zero lane falls back to `mesh.material`.

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
