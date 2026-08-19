import test from "node:test";
import assert from "node:assert/strict";

import {
  animateInstance,
  canonicalAstExample,
  classifyInstance,
  compileAndSwitch,
  computePipelineExample,
  connectFromWorker,
  createInstance,
  createVelocityColumn,
  customRenderPipelineExample,
  defaultPipelineExample,
  fluentGraphExample,
  fxNodeExportExample,
  importGltf,
  installCameraRenderDataControls,
  jsoGraphExample,
  loadCompleteScene,
  pickNearest,
  conventionalSceneHandles,
  restyleScene,
  setVelocity,
  simpleSceneShader,
  updateInstance,
} from "../examples/cookbook/index.js";

test("cookbook graph recipes produce canonical addon-owned ASTs", () => {
  const canonical = canonicalAstExample();
  assert.equal(canonical.ast.id, "shared_dag");
  assert.equal(
    (canonical.source.match(/\(ref "source" "value"\)/g) ?? []).length,
    2,
  );
  assert.deepEqual(
    [jsoGraphExample().id, fluentGraphExample().id],
    ["jso_graph", "fluent_graph"],
  );

  const fxnode = fxNodeExportExample();
  assert.equal(fxnode.kind, "yawn-render-graph");
  assert.equal(fxnode.pipelines.render.length, 4);

  const defaults = defaultPipelineExample();
  assert.deepEqual(
    defaults.pipelines.render.map(({ name }) => name),
    [
      "ground_plane",
      "gltf_standard",
      "gltf_standard_double_sided",
      "frame_out",
    ],
  );
  assert.equal(defaults.pipelines.compute.length, 1);

  const custom = customRenderPipelineExample();
  assert.equal(custom.pipelines.render[0].shader, simpleSceneShader);
  assert.match(simpleSceneShader, /@vertex fn vertex_main/);

  const compute = computePipelineExample().pipelines.compute[0];
  assert.deepEqual(
    [compute.entry, compute.dispatch],
    ["initialize", [4, 1, 1]],
  );
});

test("cookbook graph lifecycle recipe serializes and switches", async () => {
  const calls = [];
  const core = {
    compileGraph(source) {
      calls.push(["compile", source]);
      return Promise.resolve({ compiledId: [3, 4] });
    },
    switchCompiledGraph(id) {
      calls.push(["switch", id]);
      return Promise.resolve();
    },
    dropCompiledGraph(id) {
      calls.push(["drop", id]);
      return Promise.resolve();
    },
  };
  const graph = { id: "lifecycle", revision: 1, nodes: [] };
  const compiled = await compileAndSwitch(core, graph);
  assert.match(calls[0][1], /^\(yawn-graph 1/);
  assert.deepEqual(calls.slice(1), [["switch", [3, 4]]]);
});

test("cookbook mutation recipes use handles and SOA writes directly", async () => {
  const calls = [];
  const column = {
    write(slot, values) {
      calls.push(["velocity", slot, values]);
    },
  };
  const core = {
    allocateArray(layout) {
      calls.push(["allocate", layout]);
      return Promise.resolve(column);
    },
    setInstanceTransform(handle, transform) {
      calls.push(["transform", handle, transform]);
    },
    setInstanceType(handle, words) {
      calls.push(["type", handle, words]);
    },
  };
  const instance = {
    handle: [7, 2],
    setTransform(transform) {
      calls.push(["wrapped-transform", transform]);
    },
    setType(words) {
      calls.push(["wrapped-type", words]);
    },
  };
  const mesh = {
    createInstance(transform) {
      calls.push(["create", transform]);
      return Promise.resolve(instance);
    },
  };
  const transform = Array.from({ length: 16 }, (_, index) => index);
  const words = Array(16).fill(1);

  assert.equal(await createInstance(mesh, transform), instance);
  updateInstance(instance, transform, words);
  const velocity = await createVelocityColumn(core);
  setVelocity(velocity, instance, [1, 2, 3]);
  animateInstance(core, instance, transform);
  classifyInstance(core, instance, words);

  assert.deepEqual(calls.find(([name]) => name === "allocate")[1], {
    name: "instance.velocity",
    domain: "instance",
    scalar: "f32",
    lanes: 4,
  });
  assert.deepEqual(
    calls.find(([name]) => name === "velocity"),
    ["velocity", 7, [1, 2, 3, 0]],
  );
  assert.ok(calls.some(([name]) => name === "transform"));
  assert.ok(calls.some(([name]) => name === "type"));
});

test("cookbook picking and worker-to-worker recipes use public facades", async () => {
  const picked = await pickNearest(
    {
      pickRay: async () => ({
        epoch: 5,
        hits: [{ instance: [9, 3], distance: 2 }],
      }),
    },
    [0, 0, 0],
    [0, 0, -1],
  );
  assert.deepEqual(picked.hits[0].instance, [9, 3]);

  class Port extends EventTarget {
    postMessage() {}
    start() {
      queueMicrotask(() =>
        this.dispatchEvent(
          new MessageEvent("message", {
            data: { type: "soa-init", arrays: [] },
          }),
        ),
      );
    }
    terminate() {}
  }
  const core = await connectFromWorker(new Port());
  assert.equal(core.constructor.name, "YawnCore");
  core.dispose();

  assert.equal(typeof importGltf, "function");
  assert.equal(typeof installCameraRenderDataControls, "function");
  assert.equal(typeof conventionalSceneHandles, "function");
  assert.equal(typeof restyleScene, "function");
  assert.equal(typeof loadCompleteScene, "function");
});
