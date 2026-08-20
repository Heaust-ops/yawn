# Getting started

Yawn separates the data/render engine from optional scene conventions. Most applications begin with `@yawn/handles`; specialized engines can use `@yawn/core` directly.

## 1. Serve with isolation headers

`SharedArrayBuffer` requires `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. The included VitePress server already sends both.

```html
<canvas id="view"></canvas>
<script type="module" src="/src/app.ts"></script>
```

## 2. Start a Scene

```ts
import { Scene } from "@yawn/handles";

const canvas = document.querySelector<HTMLCanvasElement>("#view")!;
canvas.width = 1280;
canvas.height = 720;

const scene = new Scene(canvas, { hdr: true, fps: 60 });
await scene.ready;
```

`Scene` initializes conventional SOA rows and loads one clustered-forward HDR render graph. The core itself still starts with only its eight-float `info` row.

## 3. Add a triangle

```ts
import { Mesh, PBRMaterial } from "@yawn/handles";

const blue = new PBRMaterial(scene, {
  baseColor: [0.15, 0.55, 1, 1],
  metallic: 0.15,
  roughness: 0.4,
});
await blue.ready;

const triangle = new Mesh(scene, {
  material: blue,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await triangle.ready;
```

Constructors use worker messages only to reserve slots or rebuild the graph. Once `ready` resolves, ordinary property writes mutate shared memory.

<Playground />

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
