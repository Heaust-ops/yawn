# Render graph frontends

Every frontend ends at `@yawn/render-graph-ast`. Choose the authoring style that fits your tooling; the worker receives the same S-expression either way.

## Plain-object authoring

`graphFromObject` validates and freezes ordinary JavaScript data. Pipeline declarations, compute dispatches, nodes, and DAG references all become canonical AST fields.

```js
import { graphFromObject } from "@yawn/render-graph-js";

const graph = graphFromObject({
  id: "main",
  revision: 1,
  pipelines: { render: [scenePipeline], compute: [preparePipeline] },
  nodes: [
    {
      id: "mesh",
      state: "enabled",
      executor: { key: "mesh", version: 2 },
      parameters: {},
      inputs: {},
    },
  ],
});
```

<Playground
  id="jso-graph"
  title="A complete JSO graph"
  description="The playground compiles a plain object through AST serialization and activates the returned loadout."
/>

## Fluent authoring

Use `RenderGraph` when a small mutable builder makes generated graphs easier to read. Calling `ast()` is the immutable boundary.

```js
import { RenderGraph, ref } from "@yawn/render-graph-js";

const graph = new RenderGraph("generated", 1)
  .renderPipeline(scenePipeline)
  .node("source", "mesh", { version: 2 })
  .node("draw", "scene", {
    version: 2,
    inputs: { mesh: [ref("source", "mesh")] },
  });

const compiled = await graph.load(core);
```

## FXNode export

`@yawn/render-graph-fxnode` translates editor snapshots into the same AST. It owns editor catalog versions and diagnostic mapping; no FXNode shape crosses into core.

```js
import { adaptFxNodeSnapshot } from "@yawn/render-graph-fxnode";

const ast = adaptFxNodeSnapshot(snapshot, revision, {
  pipelines: myPipelines,
});
```

Open the <a href="/render-graph-studio/">Render Graph Studio</a> to edit an FXNode graph, compile it beside the JSO preset, and switch prepared loadouts.

## DAG references

A reference is data, not a nested expression. Point several consumers at one output to represent fan-out without repeating the source node.

```js
import { reference } from "@yawn/render-graph-ast";

const shared = reference("sceneColor", "texture");
left.inputs.color = [shared];
right.inputs.color = [shared];
```

The serializer emits `(ref "sceneColor" "texture")` wherever that edge is consumed.
