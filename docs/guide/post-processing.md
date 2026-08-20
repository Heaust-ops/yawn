# Post processing

Post-process handles insert or remove graph passes. Intermediate HDR textures are transient and compatible non-overlapping lifetimes alias the same physical allocation.

```ts
const ssao = new SSAO(scene, { amount: 0.8 });
const exposure = new DynamicExposure(scene, { exposure: 1.1 });
const grade = new ColorGrading(scene, { toneMap: "aces", amount: 1.0 });
const fxaa = new FXAA(scene);

await Promise.all([ssao.ready, exposure.ready, grade.ready, fxaa.ready]);
```

<Playground example="post" />

Every effect is optional:

```ts
await ssao.setEnabled(false);
await grade.update({ toneMap: "reinhard" });
await fxaa.dispose();
```

Also available:

```ts
const outline = new Silhouette(scene, { amount: 1 });
const edgeImage = new Edges(scene, { amount: 2, enabled: false });
await edgeImage.setEnabled(true);
```

The final present pass tone-maps HDR to the canvas. `ColorGrading` supports `aces`, `reinhard`, and `linear` tone maps.

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
