import test from "node:test";
import assert from "node:assert/strict";

import * as coreApi from "@yawn/core";
import { CameraHandle, MaterialHandles, MeshHandles } from "@yawn/mesh-handles";
import { createGraphAst, serializeGraphAst } from "@yawn/render-graph-ast";

const { YawnCore, RendererError } = coreApi;

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
    id: 5, name: "upload.renderData", domain: "fixed", scalar: "u32", lanes: 4,
    stride: 16, length: 16, capacity: 16, controlPtr: 200064, dataOffset: 64,
    byteLength: 256, layoutEpoch: 1, writable: true,
  });
  const camera = installArray(memory, {
    id: 6, name: "camera.state", domain: "fixed", scalar: "f32", lanes: 16,
    stride: 64, length: 1, capacity: 1, controlPtr: 200384, dataOffset: 64,
    byteLength: 64, layoutEpoch: 1, writable: true,
  });
  new Float32Array(memory.buffer, camera.controlPtr + camera.dataOffset, 16).set([
    4, 3, 6, 1,
    0, 0, 0, 1,
    0, 1, 0, 0,
    Math.PI / 4, 16 / 9, 0.1, 1000,
  ]);
  const material = installArray(memory, {
    id: 7, name: "material.state", domain: "fixed", scalar: "u32", lanes: 28,
    stride: 112, length: 4, capacity: 4, controlPtr: 200512, dataOffset: 64,
    byteLength: 448, layoutEpoch: 1, writable: true,
  });
  const materialFloats = new Float32Array(memory.buffer, material.controlPtr + material.dataOffset, material.byteLength / 4);
  materialFloats.set([1, 1, 1, 1, 0, 0, 0, 0, 1, 0.5, 1, 1, 0, 0.5, 1.5, 0.04], 28);
  const worker = new WorkerMock();
  const bridge = { memory, ringPtr: 0, worker, freed: false, free() { this.freed = true; } };
  const core = new YawnCore(bridge);
  worker.reply({ type: "soa-init", arrays: [transform, type, generation, meshGeneration, upload, camera, material] });
  return { memory, ring, worker, bridge, core, handles: new MeshHandles(core), transform, type, generation, upload, camera, material };
}

async function imported(fixture) {
  const loading = fixture.core.commitRenderDataUpload(fixture.core.array("upload.renderData"), 8);
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
      materials: [{ key: 1 }],
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

test("core commits generic shared render-data packets through metadata-only opcode 1", async () => {
  const fixture = setup();
  const pending = fixture.core.commitRenderDataUpload(fixture.core.array("upload.renderData"), 8);
  assert.deepEqual([...new Int32Array(fixture.memory.buffer, 64, 6)], [2, 1, 1, 5, 8, 0]);
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

test("camera is render data with no dedicated core API", () => {
  const fixture = setup();
  const camera = fixture.core.array("camera.state");
  const ringBefore = Atomics.load(fixture.ring, 5);
  const control = new Int32Array(fixture.memory.buffer, fixture.camera.controlPtr, 16);
  const sequenceBefore = Atomics.load(control, 9);
  const state = camera.read(0);
  state[0] = 5;
  state[1] = 4;
  camera.write(0, state);

  assert.equal("SharedCamera" in coreApi, false);
  assert.deepEqual(camera.read(0).slice(0, 3), [5, 4, 6]);
  assert.equal(Atomics.load(fixture.ring, 5), ringBefore);
  assert.equal(Atomics.load(control, 9), sequenceBefore + 2);
});

test("camera handle exposes conventional properties using only the shared row", () => {
  const fixture = setup();
  const camera = new CameraHandle(fixture.core);
  const ringBefore = Atomics.load(fixture.ring, 5);

  camera.update({ position: [5, 4, 7], target: [1, 0, 0], fovY: Math.PI / 3 });

  assert.deepEqual(camera.position, [5, 4, 7]);
  assert.deepEqual(camera.target, [1, 0, 0]);
  assert.ok(Math.abs(camera.fovY - Math.PI / 3) < 1e-6);
  assert.equal(Atomics.load(fixture.ring, 5), ringBefore);
  assert.throws(() => camera.update({ near: 2, far: 1 }), RangeError);
});

test("material handles mutate packed GPU material rows without messages", () => {
  const fixture = setup();
  const [material] = new MaterialHandles(fixture.core).fromImportedScene({ materials: [{ key: 1 }] });
  const ringBefore = Atomics.load(fixture.ring, 5);
  const control = new Int32Array(fixture.memory.buffer, fixture.material.controlPtr, 16);
  const sequenceBefore = Atomics.load(control, 9);

  material.update({ baseColor: [0.2, 0.4, 0.8, 1], metallic: 0.25, roughness: 0.75, ior: 2 });

  assert.equal(material.key, 1);
  assert.deepEqual(material.baseColor.map(value => Math.round(value * 10) / 10), [0.2, 0.4, 0.8, 1]);
  assert.equal(material.metallic, 0.25);
  assert.equal(material.roughness, 0.75);
  assert.equal(material.ior, 2);
  assert.equal(Atomics.load(fixture.ring, 5), ringBefore);
  assert.equal(Atomics.load(control, 9), sequenceBefore + 2);
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
  assert.deepEqual([...new Int32Array(fixture.memory.buffer, 64, 5)], [2, 9, 1, 9, 4]);
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
