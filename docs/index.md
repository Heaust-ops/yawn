---
layout: home
hero:
  name: Yawn
  text: Shared render data and a render graph.
  tagline: Two core files, one fixed arena, no built-in scene model or shader.
  actions:
    - theme: brand
      text: Open the playground
      link: /playground
features:
  - title: Shared rows
    details: Allocate an SOA row array once by message, then mutate its SAB views directly from any thread.
  - title: External graphs
    details: JSO and FXNode addons serialize DAGs to the S-expression AST consumed by the worker.
  - title: Up-front loadouts
    details: Pipelines, GPU resources, pass order, and compatible transient aliases are prepared before activation.
---

## The entire boundary

```js
const color = await core.allocateRows({
  name: "triangle.color",
  rows: 1,
  stride: 16,
  format: "f32",
});

color.write(0, [0.2, 0.65, 1, 1]);
color.row(0)[0] = 0.8; // direct SharedArrayBuffer write
await loadGraph(core, graph); // infrequent message
```

`@yawn/core` contains only the public shared-row client and its worker. The worker owns the fixed 64-byte-aligned arena, S-expression graph compiler, WebGPU loadout, and transient texture aliasing. Every scene convention and every byte of WGSL comes from an addon or application.

```text
JSO / FXNode ──▶ AST ──▶ S-expression ──▶ core worker ──▶ WebGPU
any JS thread ───────────── direct SAB row writes ────────────┘
```

The addon packages provide graph serialization, optional WGSL, glTF import directly into shared rows, and conventional camera/material/mesh handles. None of them add semantics to core.
