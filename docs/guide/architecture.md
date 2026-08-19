# How Yawn fits together

Yawn has two public boundaries: **worker communication** for small, infrequent operations and **shared render data** for values that change often. Everything else is an authoring or convenience layer outside core.

```text
JSO / fluent builder ─┐
                     ├──▶ canonical DAG AST ─▶ S-expression ─▶ render worker
FXNode snapshot ─────┘                                           │
                                                                ├─ graph compiler
glTF import worker ───── shared upload array ────────────────────┤
                                                                ├─ transient allocator
any browser thread ─── lifecycle messages ──────────────────────┤
any browser thread ─── atomic SOA writes ────────────────────────┘
```

## What core owns

`@yawn/core` owns only the protocol client for render data and render graphs. The worker behind it owns graph validation, loadout preparation, transient lifetime analysis, GPU allocation, and rendering.

Core does **not** own a camera module, scene object model, glTF parser, shader library, editor, or picking system. Camera and material values are ordinary render-data columns. Higher-level objects are optional addon views over those columns.

## The graph is the program

Every frontend must produce `@yawn/render-graph-ast` data. A node is named, while an input contains one or more `{ node, socket }` references. Reusing the same reference creates fan-out, so the format describes a DAG instead of duplicating a tree.

Render and compute pipeline declarations are part of that AST. Their WGSL and state are compiled into a prepared loadout, not linked into core.

## The SOA is the mutable scene

Shared arrays are 64-byte aligned and each row stride is a multiple of 16 bytes. The standard columns cover mesh, instance, camera, and material data. Applications can request additional mesh-, instance-, or fixed-domain arrays; domain arrays grow with the corresponding render-data capacity.

Lifecycle operations such as allocating a column, importing an asset, compiling a graph, or creating an instance cross the command boundary. A frame-rate transform, camera, classification, or material update writes the existing SAB row directly.

## Thread placement is a choice

The JS client only requires a Worker-like endpoint. A main thread can own it, or another worker can connect through a `MessagePort`. Shared descriptors can be passed to additional workers, which can then read or write the same SOA data without proxying every update through the main thread.

::: warning Cross-origin isolation is required
Serve the application with `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. `npm run examples` supplies both headers.
:::
