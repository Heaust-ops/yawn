---
layout: home
hero:
  name: Yawn raw core
  text: Worker, shared memory, and render graphs
  tagline: The protocol reference for building an alternative handles layer or editor without YawnCore or @yawn/handles.
  actions:
    - theme: brand
      text: Boot from scratch
      link: /guide/boot
    - theme: alt
      text: Worker reference
      link: /reference/worker
features:
  - title: Raw transport
    details: Start core/worker.js, correlate replies, and handle profiler events.
  - title: Shared rows
    details: Build typed views, allocate slots, and follow the dirty-lane protocol.
  - title: Graph wire format
    details: Encode, compile, and activate complete WebGPU render and compute graphs.
---

## Contract at a glance

The browser main thread owns the HTML canvas and starts the module worker. The worker initializes the WASM core and WebGPU against an `OffscreenCanvas`. All control operations are structured-clone messages. Bulk numeric state lives in one returned `SharedArrayBuffer`; graphs describe how named row arrays become GPU resources.

The application is responsible for three important contracts:

1. Correlate each reply by its `request` value; profiler messages are unsolicited.
2. Finish shared-memory writes before setting `signals[5]` (`sabDirty`).
3. Before a mutation that invalidates compiled render bundles, set `signals[6]` (`bundleDirty`), make the mutation, compile and switch a replacement graph, then let successful `switch-loadout` clear lane 6.

This site describes the current wire API, not the higher-level wrapper API.
