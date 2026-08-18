import test from "node:test";
import assert from "node:assert/strict";

import { YawnCore, RendererError } from "@yawn/core";
import { MeshHandles } from "@yawn/mesh-handles";
import { createGraphAst, serializeGraphAst } from "@yawn/render-graph-ast";

const TYPE = [0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 0x80000000, 0xffffffff];
const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

class WorkerMock extends EventTarget {
  messages = [];
  transfers = [];
  terminated = false;

  postMessage(message, transfer = []) {
    this.messages.push(message);
    this.transfers.push(transfer);
  }

  terminate() {
    this.terminated = true;
  }

  reply(data) {
    this.dispatchEvent(new MessageEvent("message", { data }));
  }
}

function installArray(memory, descriptor) {
  const control = new Int32Array(memory.buffer, descriptor.controlPtr, 16);
  control.set([
    0x414f5359,
    1,
    descriptor.id,
    { u32: 1, i32: 2, f32: 3 }[descriptor.scalar],
    descriptor.lanes,
    descriptor.stride / 4,
    descriptor.length,
    descriptor.capacity,
    { mesh: 1, instance: 2, fixed: 3 }[descriptor.domain],
    0,
    descriptor.layoutEpoch,
  ]);
  return descriptor;
}

function setup() {
  const memory = new WebAssembly.Memory({ initial: 8, maximum: 16, shared: true });
  const ring = new Int32Array(memory.buffer, 0, 16);
  ring.set([0x4e574159, 2, 1024, 40, 0, 0]);
  const transform = installArray(memory, {
    id: 1, name: "instance.transform", domain: "instance", scalar: "f32", lanes: 16,
    stride: 80, length: 16, capacity: 16, controlPtr: 196608, dataOffset: 64,
    byteLength: 1280, layoutEpoch: 1, writable: true, generationGuard: "instance",
  });
  const type = installArray(memory, {
    id: 2, name: "instance.type", domain: "instance", scalar: "u32", lanes: 16,
    stride: 80, length: 16, capacity: 16, controlPtr: 198016, dataOffset: 64,
    byteLength: 1280, layoutEpoch: 1, writable: true, generationGuard: "instance",
  });
  const generation = installArray(memory, {
    id: 3, name: "instance.generation", domain: "instance", scalar: "u32", lanes: 1,
    stride: 16, length: 16, capacity: 16, controlPtr: 199424, dataOffset: 64,
    byteLength: 256, layoutEpoch: 1, writable: false,
  });
  const meshGeneration = installArray(memory, {
    id: 4, name: "mesh.generation", domain: "mesh", scalar: "u32", lanes: 1,
    stride: 16, length: 16, capacity: 16, controlPtr: 199744, dataOffset: 64,
    byteLength: 256, layoutEpoch: 1, writable: false,
  });
  const upload = installArray(memory, {
    id: 5, name: "upload.gltf", domain: "fixed", scalar: "u32", lanes: 4,
    stride: 16, length: 16, capacity: 16, controlPtr: 200064, dataOffset: 64,
    byteLength: 256, layoutEpoch: 1, writable: true,
  });
  const worker = new WorkerMock();
  const bridge = { memory, ringPtr: 0, worker, freed: false, free() { this.freed = true; } };
  const core = new YawnCore(bridge);
  worker.reply({ type: "soa-init", arrays: [transform, type, generation, meshGeneration, upload] });
  return { memory, ring, worker, bridge, core, handles: new MeshHandles(core), transform, type, generation, upload };
}

async function imported(fixture) {
  const loading = fixture.core.commitGlbUpload(fixture.core.array("upload.gltf"), 8);
  fixture.worker.reply({
    type: "reply",
    request: 1,
    ok: true,
    result: {
      meshes: [{
        handle: [7, 3],
        defaultInstance: [8, 5],
        defaultType: TYPE,
      }],
    },
  });
  const [mesh] = fixture.handles.fromImportedScene(await loading);
  const generations = new Int32Array(
    fixture.memory.buffer,
    fixture.generation.controlPtr + fixture.generation.dataOffset,
    fixture.generation.byteLength / 4,
  );
  Atomics.store(generations, 8 * (fixture.generation.stride / 4), 5);
  return mesh;
}

test("core commits shared scene uploads through metadata-only opcode 1", async () => {
  const fixture = setup();
  const pending = fixture.core.commitGlbUpload(fixture.core.array("upload.gltf"), 8, { framing: "interior" });
  assert.deepEqual([...new Int32Array(fixture.memory.buffer, 64, 6)], [2, 1, 1, 5, 8, 1]);
  fixture.worker.reply({ type: "reply", request: 1, ok: true, result: { meshes: [] } });
  assert.deepEqual(await pending, { meshes: [] });
});

test("mesh handles are a separate conventional facade over core commands", async () => {
  const fixture = setup();
  const mesh = await imported(fixture);
  assert.deepEqual(mesh.handle, [7, 3]);
  assert.deepEqual(mesh.defaultInstance.handle, [8, 5]);

  const creating = mesh.createInstance(IDENTITY, { type: TYPE });
  const slot = new Int32Array(fixture.memory.buffer, 64 + 160, 40);
  assert.equal(slot[1], 3);
  fixture.worker.reply({ type: "reply", request: 2, ok: true, result: [4, 2] });
  const instance = await creating;
  assert.deepEqual(instance.handle, [4, 2]);
});

test("mesh handle picking wraps core protocol handles", async () => {
  const core = {
    async pickRay() {
      return { epoch: 4, hits: [{ instance: [3, 9], distance: 2 }] };
    },
  };
  const result = await new MeshHandles(core).pickRay([0, 0, 0], [1, 0, 0]);
  assert.deepEqual(result.hits[0].instance.handle, [3, 9]);
  assert.equal(result.hits[0].distance, 2);
});

test("frequent instance mutations write guarded SOA lanes without ring messages", async () => {
  const fixture = setup();
  const mesh = await imported(fixture);
  const before = Atomics.load(fixture.ring, 5);

  mesh.defaultInstance.setType(TYPE);
  mesh.defaultInstance.setTransform(IDENTITY);

  assert.equal(Atomics.load(fixture.ring, 5), before);
  const type = new Int32Array(
    fixture.memory.buffer,
    fixture.type.controlPtr + fixture.type.dataOffset,
    fixture.type.byteLength / 4,
  );
  const base = 8 * (fixture.type.stride / 4);
  assert.deepEqual([...type.slice(base, base + 16)].map(value => value >>> 0), TYPE);
  assert.equal(Atomics.load(type, base + 16) >>> 0, 5);
  assert.equal(Atomics.load(type, base + 17) >>> 0, 1);
});

test("generation columns reject stale handles and are read-only", async () => {
  const fixture = setup();
  const mesh = await imported(fixture);
  const generations = new Int32Array(
    fixture.memory.buffer,
    fixture.generation.controlPtr + fixture.generation.dataOffset,
    fixture.generation.byteLength / 4,
  );
  Atomics.store(generations, 8 * (fixture.generation.stride / 4), 6);
  assert.throws(() => mesh.defaultInstance.setTransform(IDENTITY), error => error.code === "STALE_HANDLE");
  assert.throws(() => fixture.core.array("instance.generation").write(8, [6]), error => error.code === "SOA_READ_ONLY");
});

test("custom SOA columns are allocated through the payload command", async () => {
  const fixture = setup();
  const pending = fixture.core.allocateArray({
    name: "instance.velocity", domain: "instance", scalar: "f32", lanes: 4,
  });
  await Promise.resolve();
  fixture.worker.reply({ type: "payload-ready", id: 1 });
  await Promise.resolve();
  assert.equal(new Int32Array(fixture.memory.buffer, 64, 40)[1], 11);
  const descriptor = installArray(fixture.memory, {
    id: 5, name: "instance.velocity", domain: "instance", scalar: "f32", lanes: 4,
    stride: 16, length: 16, capacity: 16, controlPtr: 200064, dataOffset: 64,
    byteLength: 256, layoutEpoch: 1, writable: true,
  });
  fixture.worker.reply({ type: "reply", request: 1, ok: true, result: descriptor });
  const array = await pending;
  array.write(3, [1, 2, 3, 4]);
  assert.deepEqual(array.read(3), [1, 2, 3, 4]);
});

test("graph compilation transfers canonical S-expressions, including pipeline declarations", async () => {
  const fixture = setup();
  const graph = createGraphAst({
    id: "compute_graph",
    revision: 1,
    pipelines: {
      compute: [{
        name: "prepare",
        shader: "@compute @workgroup_size(1) fn main() {}",
        entry: "main",
        dispatch: [1, 2, 3],
      }],
    },
    nodes: [],
  });
  const pending = fixture.core.compileGraph(serializeGraphAst(graph));
  const payload = fixture.worker.messages[0];
  assert.match(new TextDecoder().decode(payload.buffer), /^\(yawn-graph 1/);
  assert.match(new TextDecoder().decode(payload.buffer), /\"dispatch\" \(array 1 2 3\)/);
  fixture.worker.reply({ type: "payload-ready", id: 1 });
  await Promise.resolve();
  fixture.worker.reply({ type: "reply", request: 1, ok: true, result: { compiledId: [2, 3] } });
  assert.deepEqual(await pending, { compiledId: [2, 3] });
});

test("graph lifecycle remains FIFO and uses opcodes 8 and 9", async () => {
  const fixture = setup();
  const first = fixture.core.switchCompiledGraph([9, 4]);
  const second = fixture.core.dropCompiledGraph([2, 1]);
  assert.equal(Atomics.load(fixture.ring, 5), 1);
  fixture.worker.reply({ type: "reply", request: 1, ok: true });
  await first;
  await new Promise(queueMicrotask);
  assert.equal(Atomics.load(fixture.ring, 5), 2);
  const secondSlot = new Int32Array(fixture.memory.buffer, 64 + 160, 40);
  assert.equal(secondSlot[1], 8);
  fixture.worker.reply({ type: "reply", request: 2, ok: true });
  await second;
});

test("transport failures reject pending work and dispose owned resources", async () => {
  const fixture = setup();
  const pending = fixture.core.dropCompiledGraph([1, 1]);
  fixture.worker.dispatchEvent(new Event("error"));
  await assert.rejects(pending, error => error instanceof RendererError && error.code === "WORKER_ERROR");
  assert.equal(fixture.worker.terminated, true);
  assert.equal(fixture.bridge.freed, true);
});
