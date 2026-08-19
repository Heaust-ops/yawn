# Core and render data

`@yawn/core` is a protocol client. It manages the command ring, payload handshakes, graph lifecycle, and typed views over shared render-data arrays.

## Fast-path shared writes

Standard instance APIs validate `[slot, generation]`, then write the corresponding guarded SOA row. They do not enqueue a renderer command.

```js
core.setInstanceTransform(instanceHandle, matrix);
core.setInstanceType(instanceHandle, sixteenU32Words);
```

The convenience `Instance` methods in `@yawn/mesh-handles` call exactly these APIs.

<Playground
  id="shared-animation"
  title="Direct shared-memory animation"
  description="A requestAnimationFrame loop updates one instance transform without per-frame messages."
/>

## Request an SOA column

Array creation is intentionally an infrequent worker command. Choose a domain so core can keep the array's logical length synchronized with fixed, mesh, or instance capacity.

```js
const velocity = await core.allocateArray({
  name: "instance.velocity",
  domain: "instance",
  scalar: "f32",
  lanes: 4,
});

velocity.write(instanceHandle[0], [1, 0, 0, 0]);
```

Every stride is a multiple of 16 bytes, keeping rows suitable for vectorized consumers. `SharedSoaArray` uses atomic lane access and refreshes its typed views when shared WASM memory grows.

<Playground
  id="custom-soa"
  title="Application-owned velocity rows"
  description="Allocate an instance-domain column and populate one SIMD-width row per live instance."
/>

## Share a column with another worker

Use `share()` only during setup. It returns the shared backing buffer and wire descriptor needed to construct a compatible view in another package or worker.

```js
const { buffer, descriptor } = velocity.share();
simulationWorker.postMessage({ type: "velocity-layout", buffer, descriptor });
```

The `SharedArrayBuffer` is shared, not transferred. Once installed, the simulation worker should mutate rows directly and reserve messages for layout or lifecycle changes.

## Graph lifecycle

Core accepts one graph format: the serialized S-expression produced by `@yawn/render-graph-ast`.

```js
const compiled = await core.compileGraph(serializedAst);
await core.switchCompiledGraph(compiled.compiledId);
await core.dropCompiledGraph(oldCompiledId);
```

Graph operations are serialized by the client so compile, switch, and drop cannot race each other on one core instance.
