# Graph authoring recipes

All three authoring styles produce the same canonical immutable AST.

## 01 — Canonical DAG AST

Create references separately from nodes. Reusing `shared` makes one output fan out to two consumers.

```js
import { createGraphAst, reference, serializeGraphAst } from "@yawn/render-graph-ast";

const expression = (id, inputs = {}) => ({
  id,
  state: "enabled",
  executor: { key: "and", version: 2 },
  parameters: {},
  inputs,
});
const shared = reference("source", "value");
const ast = createGraphAst({
  id: "shared_dag",
  revision: 1,
  nodes: [
    expression("source"),
    expression("left", { inputs: [shared] }),
    expression("right", { inputs: [shared] }),
  ],
});
const source = serializeGraphAst(ast);
```

## 02 — Plain JavaScript object graph

Let `@yawn/render-graph-js` canonicalize an ordinary object when application code does not need to manipulate AST internals.

```js
import { graphFromObject } from "@yawn/render-graph-js";

const graph = graphFromObject({
  id: "jso_graph",
  revision: 1,
  nodes: [{
    id: "mesh",
    state: "enabled",
    executor: { key: "mesh", version: 2 },
    parameters: {},
    inputs: {},
  }],
});
```

<Playground id="jso-graph" title="Compile a complete JSO graph" />

## 03 — Fluent graph builder

Use the chainable facade for generated graphs, then call `ast()` or `load(core)` at the boundary.

```js
import { RenderGraph, ref } from "@yawn/render-graph-js";

const ast = new RenderGraph("fluent_graph", 1)
  .node("source", "and", { version: 2 })
  .node("consumer", "not", {
    inputs: { operand: [ref("source", "value")] },
  })
  .ast();
```

## 04 — Export an FXNode snapshot

Keep editor schemas in the FXNode addon. Attach external pipelines during export so the resulting AST is a self-contained loadout description.

```js
import { defaultPipelines } from "@yawn/default-pipelines";
import { adaptFxNodeSnapshot } from "@yawn/render-graph-fxnode";

const ast = adaptFxNodeSnapshot(snapshot, 1, {
  pipelines: defaultPipelines,
});
```

Use the <a href="/render-graph-studio/">Render Graph Studio</a> for the interactive FXNode version of this recipe.
