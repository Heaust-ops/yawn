# Your first node

## What you will build

A Canvas editor containing one numeric value node, matching the repository's executable minimal example.

## Prerequisites

Install `fxnode`. Give the canvas non-zero CSS dimensions (the attributes also provide a useful fallback), then prepare a browser host that measures it and forwards input:

```html
<canvas id="graph" width="1000" height="560" style="width: 100%; height: 560px"></canvas>
```

The repository's [small host implementation](https://github.com/Heaust-ops/fxnode/blob/main/examples/shared/browser-host.ts) contains viewport, resize, and input wiring; see [browser host](../guides/browser-host) for its contract.

## Checkpoint

Your canvas has non-zero CSS dimensions and your host has produced `initialViewport`.

## 1. Prepare application-owned definitions

`theme`, `minimalStyles`, `numberSocket`, and `valueNode` below are **application-owned definitions**, not fxnode globals. The socket and node are exported as `[id, definition]` tuples so they can be passed directly to the composition methods. Define or import them before bootstrap; the executable [definition file](https://github.com/Heaust-ops/fxnode/blob/main/examples/minimal/definition.ts) is the compact reference.

## 2. Bootstrap in dependency order

Create the shared root first, then install composition dependencies in order: theme, header styles, sockets, nodes, and finally graph state. Attach the canvas view after that bootstrap.

```ts
import { createFxNode } from "fxnode";

const root = await createFxNode({
  applicationId: "my.first.editor",
  applicationVersion: 1,
  resources: {},
});
await root.setTheme(theme);
await root.setHeaderStyles(minimalStyles);
await root.composeSocket(...numberSocket);
await root.composeNode(...valueNode);
await root.setState({ graphId: "first", catalogVersion: 1, nodes: [], links: [], metadata: {} });
const view = await root.attachView({ canvas, viewport: host.initialViewport });
host.attach(root, view);
await view.addNode({ nodeId: "value", typeId: valueNode[0], viewPosition: { x: 360, y: 190 } });
await view.whenRendered();
```

**Checkpoint:** a “Number Value” node is visible. The host is attached only after setup, and the view's `whenRendered()` confirms the committed node reached a frame.

### Why this order?

The worker validates every definition against current authority. `setState` is last so known nodes bind against the complete composition. Host attachment follows bootstrap so input cannot race setup.

## 3. Clean up

Remove application listeners, call `host.destroy()`, await `view.detach()`, then call `root.destroy()` on unmount or `pagehide`. Also destroy a late-created root if teardown wins a startup race. The complete source demonstrates that guard.

## Complete example

The complete executable source is [`examples/minimal/main.ts`](https://github.com/Heaust-ops/fxnode/blob/main/examples/minimal/main.ts), with its [`definition.ts`](https://github.com/Heaust-ops/fxnode/blob/main/examples/minimal/definition.ts).

## Related concepts

[Composition](../concepts/composition) and [worker authority](../concepts/worker-authority).

## Relevant API

[`createFxNode`](/reference/generated/fxnode/functions/createFxNode), [`FxNode`](/reference/generated/fxnode/interfaces/FxNode), and [`FxNodeView`](/reference/generated/fxnode/interfaces/FxNodeView).

## Next

Build a richer [Color Balance node](./color-balance).
