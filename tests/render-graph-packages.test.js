import test from "node:test";
import assert from "node:assert/strict";

import {
  GraphAstError,
  createGraphAst,
  reference,
  serializeGraphAst,
} from "@yawn/render-graph-ast";
import { RenderGraph, graphFromObject, loadGraph } from "@yawn/render-graph-js";
import { defaultPipelines } from "@yawn/default-pipelines";

const node = (id, inputs = {}) => ({
  id,
  state: "enabled",
  executor: { key: "and", version: 2 },
  parameters: {},
  inputs,
});

test("JSO and builder frontends export the same canonical AST", () => {
  const description = {
    id: "shared_dag",
    revision: 3,
    pipelines: {
      compute: [{
        name: "prepare",
        shader: "@compute @workgroup_size(1) fn main() {}",
        entry: "main",
        dispatch: [1, 1, 1],
      }],
    },
    nodes: [
      node("source"),
      node("left", { inputs: [reference("source", "value")] }),
      node("right", { inputs: [reference("source", "value")] }),
    ],
  };
  const objectAst = graphFromObject(description);
  const builderAst = new RenderGraph("shared_dag", 3)
    .computePipeline(description.pipelines.compute[0])
    .node("source", "and", { version: 2 })
    .node("left", "and", { version: 2, inputs: description.nodes[1].inputs })
    .node("right", "and", { version: 2, inputs: description.nodes[2].inputs })
    .ast();

  assert.deepEqual(builderAst, objectAst);
  const source = serializeGraphAst(objectAst);
  assert.equal((source.match(/\(ref "source" "value"\)/g) ?? []).length, 2);
  assert.match(source, /^\(yawn-graph 1/);
  assert.equal(source.trimEnd().endsWith("))"), true);
});

test("canonical AST is immutable and rejects duplicate declarations", () => {
  const ast = createGraphAst({ id: "immutable", revision: 1, nodes: [] });
  assert.equal(Object.isFrozen(ast), true);
  assert.equal(Object.isFrozen(ast.nodes), true);
  assert.throws(
    () => createGraphAst({
      id: "duplicates",
      revision: 1,
      pipelines: {
        render: [{ name: "same", shader: "", vertexEntry: "vs_main", fragmentEntry: "fs_main" }],
        compute: [{ name: "same", shader: "", entry: "main", dispatch: [1, 1, 1] }],
      },
      nodes: [],
    }),
    error => error instanceof GraphAstError && error.code === "AST_PIPELINE_DUPLICATE",
  );
});

test("JSO addon owns AST serialization and graph loading", async () => {
  const calls = [];
  const core = { compileGraph(source) { calls.push(source); return Promise.resolve({ compiledId: [1, 2] }); } };
  const description = { id: "loaded", revision: 1, nodes: [] };
  assert.deepEqual(await loadGraph(core, description), { compiledId: [1, 2] });
  assert.match(calls[0], /^\(yawn-graph 1/);
  await new RenderGraph("builder", 1).load(core);
  assert.match(calls[1], /\(id "builder"\)/);
});

test("optional pipelines carry every shader and compute declaration outside core", () => {
  assert.deepEqual(defaultPipelines.render.map(({ name }) => name), [
    "ground_plane", "gltf_standard", "gltf_standard_double_sided", "frame_out",
  ]);
  assert.match(defaultPipelines.render[1].shader, /@vertex/);
  assert.match(defaultPipelines.render[3].shader, /@fragment/);
  assert.match(defaultPipelines.compute[0].shader, /@compute/);
  assert.deepEqual(defaultPipelines.compute[0].dispatch, [1, 1, 1]);
});
