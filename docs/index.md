---
layout: home

hero:
  name: Yawn
  text: Build the graph. Share the data.
  tagline: A worker-native WebGPU renderer where infrequent lifecycle commands use messages and hot render data lives in SIMD-aligned shared memory.
  actions:
    - theme: brand
      text: Build your first scene
      link: /guide/first-scene
    - theme: alt
      text: Open the playground
      link: /../playground/

features:
  - title: One graph boundary
    details: JSO, a fluent builder, and FXNode all export the same immutable DAG AST and S-expression wire format.
  - title: Shared render data
    details: Meshes, instances, camera state, materials, and user columns use aligned SOA rows backed by shared WASM memory.
  - title: External programs
    details: WGSL, render pipelines, and compute passes travel with a graph loadout; core ships no scene shader.
  - title: Worker-native
    details: The same core client runs on the browser main thread or another worker through a Worker-like endpoint.
  - title: Up-front loadouts
    details: Graph compilation culls dead work, aliases compatible transients, coalesces passes, and prepares resources before activation.
  - title: Optional conveniences
    details: glTF import, mesh handles, material properties, camera controls, and picking stay in focused addons.
---

<Playground
  id="first-scene"
  title="Your first Yawn scene"
  description="The preview imports procedural glTF through a worker, activates a graph, and renders shared instance data."
/>
