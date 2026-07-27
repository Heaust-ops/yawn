# Live composition

## What you will build

An editor that replaces a version-1 node definition with version 2 and migrates its graph instance atomically.

## Prerequisites and checkpoint

Understand [composition](../concepts/composition). Start with the v1 node visible and retain the receipt's `revision`.

## 1. Acquire the v1 revision

```ts
import type { FxNode, FxNodeView } from "fxnode";

const v1Receipt = await api.composeNode("example.live.parameter", liveNodeV1);
let revision = v1Receipt.revision;
```

This is an **excerpt**: await socket dependencies first, compose v1, call `setState`, attach a view, add its instance through that view, attach the host, and render. **Checkpoint:** v1 is visible and `revision` came from its receipt—not a guessed constant.

## 2. Replace it using the v2 receipt

```ts
async function upgrade(root: FxNode, view: FxNodeView) {
  const v2Receipt = await root.composeNode("example.live.parameter", liveNodeV2, {
    expectedRevision: revision,
  });
  revision = v2Receipt.revision;
  await view.whenRendered();
  return v2Receipt;
}
```

Invoke this on an explicit host action. **Checkpoint:** inspect `v2Receipt.status`, `graphChanged`, `graphVersion`, and updated `revision`; the migrated v2 node is visible. Clean up the button/page listeners, host, and API on teardown.

### Why?

Composition revision and graph version are separate concurrency domains. A committed rebind can advance both; a no-op advances neither. Compare-and-swap prevents two writers from assuming the same authority.

## Complete example

See the working [`examples/live-composition/main.ts`](https://github.com/Heaust-ops/fxnode/blob/main/examples/live-composition/main.ts) and its [definitions](https://github.com/Heaust-ops/fxnode/blob/main/examples/live-composition/definitions.ts).

## Related concepts / relevant API / next

Read [graph state and events](../concepts/graph-state-and-events) and [`CompositionReceipt`](/reference/generated/fxnode/type-aliases/CompositionReceipt), then plan [lifecycle cleanup](../guides/rendering-and-lifecycle).
