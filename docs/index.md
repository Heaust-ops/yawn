---
layout: home
hero:
  name: Yawn
  text: Render graphs over shared data.
  tagline: A small Rust/WASM core with optional conventional TypeScript handles.
  actions:
    - theme: brand
      text: Start the tutorial
      link: /guide/getting-started
    - theme: alt
      text: Open playground
      link: /playground
features:
  - title: Hot state stays shared
    details: Transform, material, light, and camera changes are direct SharedArrayBuffer writes from any thread.
  - title: Graph-authored GPU work
    details: WGSL, pipelines, compute, HDR, and post effects live in one externally supplied DAG loadout.
  - title: Conventional when wanted
    details: The handles addon supplies Scene, Mesh, materials, lights, glTF import, and BVH picking without adding core semantics.
---

## The shortest useful scene

```ts
import { Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(document.querySelector("canvas"), { hdr: true });
await scene.ready;

const material = new PBRMaterial(scene, { baseColor: [0.2, 0.7, 1, 1] });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.7, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

mesh.position[0] = 0.25; // direct SAB mutation
```

`Scene` installs one HDR clustered-forward loadout. Adding compute, custom shaders, textures, or post effects rebuilds that same loadout; changing values already present in shared rows does not send a message.

```text
handles ──▶ graph AST ──▶ S-expression ──▶ core worker ──▶ Rust/WebGPU
any JS thread ─────────────── direct SAB row writes ────────────────┘
```
