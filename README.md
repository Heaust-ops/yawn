# Yawn

Yawn Core is two things: a generic structure-of-arrays arena in a `SharedArrayBuffer`, and a Rust render graph that turns externally supplied WGSL into an up-front WebGPU loadout.

```text
JSO or FXNode → AST → S-expression → worker messages → Rust/WebGPU
                                                   ↑
any thread → direct shared row writes ─────────────┘
```

The arena starts with only one eight-float `info` row for frame timing and direct SAB render skipping. Messages create or delete other `{ name, rows, stride, format }` arrays, allocate named slots, compile graphs, switch loadouts, and control render pacing. Existing render data is changed by writing `f32`, `u32`, or `i32` rows directly. Allocations are 64-byte aligned, row strides are multiples of 16 bytes, and compatible non-overlapping transient textures share physical allocations.

WGSL, pipelines, glTF import, and conventional mesh/camera/material handles live in `addons/`; core contains no shader or scene model.

```sh
npm start
```

This opens the docs. The complete runnable example is at `/playground`.
