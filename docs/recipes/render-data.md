# Asset and render-data recipes

Bulk data moves through shared storage. Small descriptors and lifecycle decisions move through messages.

## 09 — Import glTF in a worker

```js
import { GltfImporter } from "@yawn/gltf-import";
import { MeshHandles } from "@yawn/mesh-handles";

const importer = new GltfImporter(core);
try {
  const imported = await importer.load(url);
  const meshes = new MeshHandles(core).fromImportedScene(imported);
} finally {
  importer.dispose();
}
```

<Playground id="gltf-worker" title="Shared-memory glTF import" />

## 10 — Create and mutate mesh instances

Creating or destroying an instance is lifecycle communication. Mutating an existing transform or type is a generation-guarded shared write.

```js
const identity = [
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1,
];

const instance = await mesh.createInstance(identity);
instance.setTransform(nextTransform);
instance.setType(sixteenU32Words);
```

## 11 — Add a custom SOA column

Select the instance domain to keep row count aligned with instance capacity. Use four lanes for one SIMD-width velocity row.

```js
const velocity = await core.allocateArray({
  name: "instance.velocity",
  domain: "instance",
  scalar: "f32",
  lanes: 4,
});

velocity.write(instance.handle[0], [x, y, z, 0]);
```

<Playground id="custom-soa" title="Allocate instance velocity data" />
