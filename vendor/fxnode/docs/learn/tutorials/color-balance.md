# Build a Color Balance editor

## What you will build

A focused editor with float/color socket types and the repository's grading-wheel Color Balance definition.

## Prerequisites and checkpoint

Complete [your first node](./first-node). Confirm the empty editor renders before adding the two socket definitions.

## 1. Install dependencies

After `createFxNode`, install theme and styles, then compose `float`, compose `color`, and compose the node. Make `setState` the final bootstrap state call; attach a view, add the node through that view, attach the host, and await the view's `whenRendered()`.

```ts
import { createFxNode } from "fxnode";

const root = await createFxNode({
  applicationId: "color.balance",
  applicationVersion: 1,
  resources: {},
});
await root.setTheme(theme);
await root.setHeaderStyles(styles);
await root.composeSocket(...floatSocket);
await root.composeSocket(...colorSocket);
await root.composeNode(...colorBalanceNode);
await root.setState({ graphId: "color-balance", catalogVersion: 1, nodes: [], links: [], metadata: {} });
const view = await root.attachView({ canvas, viewport });
host.attach(root, view);
await view.addNode({
  nodeId: "color-balance",
  typeId: colorBalanceNode[0],
  viewPosition: { x: 300, y: 40 },
});
await view.whenRendered();
```

This is an **excerpt**: `canvas`, `host`, `viewport`, theme, styles, sockets, and node definition are application-owned setup shown in the working source.

**Checkpoint:** the Color Balance node and its three grading wheels are visible and interactive.

### Why?

Definitions refer to styles and sockets, so dependencies must exist first. The widget edits graph data; fxnode does not perform color correction or execute the graph.

## 2. Attach, verify, and clean up

Attachment starts DOM input forwarding; the view's `whenRendered()` establishes a visible-frame checkpoint. On teardown remove listeners, run `host.destroy()`, await `view.detach()`, and call `root.destroy()` (including startup failure and startup/teardown races).

## Complete example

See [`examples/color-balance/main.ts`](https://github.com/Heaust-ops/fxnode/blob/main/examples/color-balance/main.ts) and the shared [node definition](https://github.com/Heaust-ops/fxnode/blob/main/examples/shared/nodes/color-balance.ts).

## Related concepts / relevant API / next

Read [composition](../concepts/composition), then inspect [`FxNode.composeNode`](/reference/generated/fxnode/interfaces/FxNode#composenode) and continue to [live composition](./live-composition).
