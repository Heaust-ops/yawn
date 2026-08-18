import test from "node:test";
import assert from "node:assert/strict";

import { GltfImporter } from "@yawn/gltf-import";
import { writeSharedUpload } from "../addons/gltf-import/src/shared-upload.js";

class WorkerMock extends EventTarget {
  messages = [];
  terminated = false;
  postMessage(message) { this.messages.push(message); }
  reply(data) { this.dispatchEvent(new MessageEvent("message", { data })); }
  terminate() { this.terminated = true; }
}

const tick = () => new Promise(resolve => setImmediate(resolve));

test("glTF addon stages fetched bytes in shared SOA and commits only metadata", async () => {
  const memory = new SharedArrayBuffer(4096);
  const descriptor = {
    id: 9,
    name: "upload.gltf",
    domain: "fixed",
    scalar: "u32",
    lanes: 4,
    stride: 16,
    length: 2,
    capacity: 2,
    controlPtr: 0,
    dataOffset: 64,
    byteLength: 32,
    layoutEpoch: 1,
    writable: true,
  };
  new Int32Array(memory, 0, 16).set([0x414f5359, 1, 9, 1, 4, 4, 2, 2, 3]);
  const array = { share: () => ({ buffer: memory, descriptor }) };
  const calls = [];
  const core = {
    async allocateArray(layout) { calls.push(["allocate", layout]); return array; },
    async commitGlbUpload(value, byteLength, options) {
      calls.push(["commit", value, byteLength, options]);
      return { meshes: [] };
    },
  };
  const worker = new WorkerMock();
  const importer = new GltfImporter(core, { workerFactory: () => worker });
  const loading = importer.load("https://example.test/scene.glb", { framing: "interior" });
  await tick();
  assert.deepEqual(worker.messages[0], {
    type: "load",
    request: 1,
    url: "https://example.test/scene.glb",
  });

  worker.reply({ type: "allocate", request: 1, byteLength: 20 });
  await tick();
  assert.deepEqual(calls[0], ["allocate", {
    name: "upload.gltf", domain: "fixed", scalar: "u32", lanes: 4, stride: 16, length: 2,
  }]);
  assert.equal(worker.messages[1].buffer, memory);
  assert.equal(worker.messages[1].descriptor, descriptor);
  assert.equal(Object.hasOwn(worker.messages[1], "bytes"), false);

  const bytes = Uint8Array.from({ length: 20 }, (_, index) => index + 1);
  writeSharedUpload(memory, descriptor, bytes);
  worker.reply({ type: "ready", request: 1, byteLength: bytes.byteLength });
  assert.deepEqual(await loading, { meshes: [] });
  assert.deepEqual(new Uint8Array(memory, 64, 20), bytes);
  assert.deepEqual(calls[1], ["commit", array, 20, { framing: "interior" }]);
  importer.dispose();
  assert.equal(worker.terminated, true);
});
