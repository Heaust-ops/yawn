import test from "node:test";
import assert from "node:assert/strict";
import { SnapshotReader, SnapshotProtocolError } from "../static/render-data-snapshot.js";
import { DerivedBvh } from "../static/bvh-core.js";
import { RendererClient } from "../static/renderer-client.js";

const align16 = value => (value + 15) & ~15;
const componentCounts = [1, 1, 1, 3, 3, 1, 1, 1, 1, 1, 16, 3, 3, 1];
const scalarTypes = [1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 2, 2, 2, 1];

function snapshotFixture({ instances = 1 } = {}) {
  const memory = new WebAssembly.Memory({ initial: 4, maximum: 8, shared: true });
  const words = new Uint32Array(memory.buffer);
  const control = new Int32Array(memory.buffer, 0, 64);
  const ptr = 256;
  const counts = [1, 1, 1, 1, 1, ...Array(9).fill(instances)];
  const offsets = [];
  let cursor = 512;
  for (let i = 0; i < 14; i++) {
    offsets.push(cursor);
    cursor = align16(cursor + counts[i] * componentCounts[i] * 4);
  }
  control.set([0x504e5359, 1, 256, 3, 64, 1, 1, 2, 1, 0, 7, 0, 4, 1, 0, 0]);
  control.set([2, 1, 1, ptr, cursor, 7, 0, 1, instances, 1, 64, 0, 0, 0, 0, 0], 16);
  const blob = new Uint32Array(memory.buffer, ptr, cursor / 4);
  blob.set([0x31534452, 1, 64, cursor, 1, 7, 0, 14, 64, 32, 1, instances, 0x01020304, 3, 0, 0]);
  for (let i = 0; i < 14; i++) {
    blob.set([i + 1, scalarTypes[i], offsets[i], counts[i], componentCounts[i], componentCounts[i] * 4, 4, 0], 16 + i * 8);
  }
  const stream = i => scalarTypes[i] === 2
    ? new Float32Array(memory.buffer, ptr + offsets[i], counts[i] * componentCounts[i])
    : new Uint32Array(memory.buffer, ptr + offsets[i], counts[i] * componentCounts[i]);
  stream(0)[0] = 4; stream(1)[0] = 2; stream(2)[0] = 1;
  stream(3).set([-1, -1, -1]); stream(4).set([1, 1, 1]);
  for (let i = 0; i < instances; i++) {
    stream(5)[i] = 10 + i; stream(6)[i] = 3; stream(7)[i] = 4; stream(8)[i] = 2;
    stream(9)[i] = 1; stream(13)[i] = 1;
    stream(11).set([i * 4, -1, -1], i * 3); stream(12).set([i * 4 + 2, 1, 1], i * 3);
  }
  return { memory, control, ptr, cursor };
}

test("snapshot reader validates and pins an exact ABI snapshot", () => {
  const f = snapshotFixture();
  const reader = new SnapshotReader(f.memory, 0);
  const result = reader.transaction(snapshot => ({
    epoch: snapshot.epoch,
    slot: snapshot.streams.instanceSlot[0],
    min: [...snapshot.streams.instanceWorldMin],
  }), 1);
  assert.deepEqual(result, { epoch: 1, slot: 10, min: [0, -1, -1] });
  assert.equal(Atomics.load(f.control, 16), 0);
});

test("snapshot reader rejects malformed control and WRITING slots", () => {
  const bad = snapshotFixture();
  bad.control[0] = 0;
  assert.throws(() => new SnapshotReader(bad.memory, 0), SnapshotProtocolError);
  const writing = snapshotFixture();
  Atomics.store(writing.control, 16, 1);
  assert.equal(new SnapshotReader(writing.memory, 0).transaction(() => true, 1), null);
});

test("snapshot reader refreshes memory after pinning", () => {
  const f = snapshotFixture();
  const reader = new SnapshotReader(f.memory, 0);
  const original = Atomics.compareExchange;
  let grew = false;
  Atomics.compareExchange = (...args) => {
    const result = original(...args);
    if (!grew && args[1] === 16 && result === 2) { grew = true; f.memory.grow(1); }
    return result;
  };
  try {
    assert.equal(reader.transaction(snapshot => snapshot.instanceCount, 1), 1);
    assert.equal(grew, true);
  } finally {
    Atomics.compareExchange = original;
  }
});

function bvhSnapshot({ pickable = [1, 1], shifted = false } = {}) {
  const count = 2;
  return { instanceCount: count, streams: {
    instanceSlot: Uint32Array.from([5, 6]), instanceGeneration: Uint32Array.from([1, 1]),
    instanceMeshSlot: Uint32Array.from([2, 2]), instanceMeshGeneration: Uint32Array.from([4, 4]),
    instancePickable: Uint32Array.from(pickable),
    instanceWorldMin: Float32Array.from(shifted ? [10, -1, -1, 4, -1, -1] : [2, -1, -1, 4, -1, -1]),
    instanceWorldMax: Float32Array.from(shifted ? [12, 1, 1, 6, 1, 1] : [3, 1, 1, 6, 1, 1]),
  }};
}

test("BVH preserves topology for visibility/refit and reports world distances", () => {
  const bvh = new DerivedBvh();
  bvh.update(bvhSnapshot());
  assert.equal(bvh.rebuilds, 1);
  assert.equal(bvh.pick([0, 0, 0], [2, 0, 0], Infinity, 2)[0].distance, 2);
  bvh.update(bvhSnapshot({ pickable: [0, 1], shifted: true }));
  assert.equal(bvh.rebuilds, 1);
  assert.equal(bvh.refits, 1);
  assert.deepEqual(bvh.pick([0, 0, 0], [1, 0, 0], Infinity, 2).map(hit => hit.slot), [6]);
});

class WorkerMock extends EventTarget {
  messages = []; terminated = false;
  postMessage(message) { this.messages.push(message); }
  terminate() { this.terminated = true; }
  reply(data) { this.dispatchEvent(new MessageEvent("message", { data })); }
}

test("renderer pick returns gated instances and exact epoch", async () => {
  const scene = snapshotFixture();
  const ring = 8192;
  const ringHeader = new Int32Array(scene.memory.buffer, ring, 16);
  ringHeader.set([0x4e574159, 1, 1024, 24]);
  const rendererWorker = new WorkerMock(), bvhWorker = new WorkerMock();
  const bridge = { memory: scene.memory, ringPtr: ring, worker: rendererWorker, workerFactory: () => bvhWorker, free() {} };
  const client = new RendererClient(bridge);
  rendererWorker.reply({ type: "snapshot-init", controlPtr: 0, controlVersion: 1, schemaVersion: 1 });
  rendererWorker.reply({ type: "snapshot-published", epoch: 1 });
  const picking = client.pickRay([0, 0, 0], [1, 0, 0]);
  const request = bvhWorker.messages.find(message => message.type === "pick");
  bvhWorker.reply({ type: "pick", request: request.request, epoch: 1, stale: false, hits: [{ slot: 10, generation: 3, distance: 2 }] });
  const result = await picking;
  assert.equal(result.epoch, 1);
  assert.equal(result.hits[0].distance, 2);
  assert.equal(typeof result.hits[0].instance.setVisible, "function");
  client.dispose();
  assert.equal(bvhWorker.terminated, true);
});
