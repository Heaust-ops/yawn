# Yawn

Yawn Core is two things: a generic structure-of-arrays arena in a `SharedArrayBuffer`, and a render-graph worker that turns externally supplied WGSL into an up-front WebGPU loadout.

```text
JSO or FXNode → AST → S-expression → graph worker → WebGPU
                                     ↑
any thread → direct shared row writes ┘
```

Messages allocate `{ name, rows, stride, format }` arrays and load graphs. Existing render data is changed by writing `f32`, `u32`, or `i32` rows directly. Allocations are 64-byte aligned, row strides are multiples of 16 bytes, and compatible non-overlapping transient textures share physical allocations.

WGSL, pipelines, glTF import, and conventional mesh/camera/material handles live in `addons/`; core contains no shader or scene model.

```sh
npm start
```

This opens the docs. The complete runnable example is at `/playground`.
