export const SNAPSHOT = Object.freeze({
  MAGIC: 0x504e5359, BLOB_MAGIC: 0x31534452, VERSION: 1, BYTES: 256,
  SLOTS: 3, SLOT_BYTES: 64, SCHEMA: 1, INIT: 0, OPEN: 1, FAILED: 2,
  CLOSED: 3, FREE: 0, WRITING: 1, READY: 2, READING: 3,
});
export const STREAM_NAMES = ["meshSlot", "meshGeneration", "meshFlags", "meshLocalMin", "meshLocalMax", "instanceSlot", "instanceGeneration", "instanceMeshSlot", "instanceMeshGeneration", "instanceFlags", "instanceModel", "instanceWorldMin", "instanceWorldMax", "instancePickable"];
const COMPONENTS = [1, 1, 1, 3, 3, 1, 1, 1, 1, 1, 16, 3, 3, 1];
const SCALARS = [1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 2, 2, 2, 1];
const STRIDES = COMPONENTS.map(n => n * 4);

export class SnapshotProtocolError extends Error {
  constructor(code) { super(code); this.code = code; this.name = "SnapshotProtocolError"; }
}

const bad = code => { throw new SnapshotProtocolError(code); };
const add = (a, b) => { const n = a + b; if (!Number.isSafeInteger(n) || n > 0xffffffff) bad("BAD_RANGE"); return n; };
const mul = (a, b) => { const n = a * b; if (!Number.isSafeInteger(n) || n > 0xffffffff) bad("BAD_RANGE"); return n; };

export class SnapshotReader {
  constructor(memory, controlPtr) {
    if (!memory || !(memory.buffer instanceof SharedArrayBuffer)) bad("BAD_MEMORY");
    if (!Number.isInteger(controlPtr) || controlPtr < 0 || controlPtr % 64 || add(controlPtr, 256) > memory.buffer.byteLength) bad("BAD_CONTROL_POINTER");
    this.memory = memory; this.controlPtr = controlPtr; this.buffer = null;
    this.refresh(); this.validateControl();
  }
  refresh() {
    if (this.buffer === this.memory.buffer) return;
    this.buffer = this.memory.buffer;
    if (add(this.controlPtr, 256) > this.buffer.byteLength) bad("BAD_CONTROL_POINTER");
    this.control = new Int32Array(this.buffer, this.controlPtr, 64);
  }
  validateControl() {
    const h = this.control;
    if ((Atomics.load(h, 0) >>> 0) !== SNAPSHOT.MAGIC) bad("BAD_MAGIC");
    if ((Atomics.load(h, 1) >>> 0) !== SNAPSHOT.VERSION) bad("BAD_VERSION");
    if ((Atomics.load(h, 2) >>> 0) !== SNAPSHOT.BYTES || (Atomics.load(h, 3) >>> 0) !== SNAPSHOT.SLOTS || (Atomics.load(h, 4) >>> 0) !== SNAPSHOT.SLOT_BYTES || (Atomics.load(h, 5) >>> 0) !== SNAPSHOT.SCHEMA) bad("BAD_LAYOUT");
    const lifecycle = Atomics.load(h, 6) >>> 0;
    if (lifecycle > SNAPSHOT.CLOSED) bad("BAD_LIFECYCLE");
    if ((Atomics.load(h, 15) >>> 0) !== 0) bad("BAD_RESERVED");
  }
  latest() {
    this.refresh(); this.validateControl();
    for (let tries = 0; tries < 16; tries++) {
      const a = Atomics.load(this.control, 7) >>> 0;
      if (a & 1) continue;
      const lifecycle = Atomics.load(this.control, 6) >>> 0;
      const value = { lifecycle, epoch: Atomics.load(this.control, 8) >>> 0, slot: Atomics.load(this.control, 9) >>> 0, revisionLo: Atomics.load(this.control, 10) >>> 0, revisionHi: Atomics.load(this.control, 11) >>> 0, wasmPages: Atomics.load(this.control, 12) >>> 0, layoutEpoch: Atomics.load(this.control, 13) >>> 0, error: Atomics.load(this.control, 14) >>> 0 };
      const b = Atomics.load(this.control, 7) >>> 0;
      if (a === b && !(b & 1)) {
        if (lifecycle === SNAPSHOT.FAILED) bad("SNAPSHOT_FAILED");
        if (lifecycle === SNAPSHOT.CLOSED) bad("SNAPSHOT_CLOSED");
        if (lifecycle !== SNAPSHOT.INIT && lifecycle !== SNAPSHOT.OPEN) bad("BAD_LIFECYCLE");
        if (value.wasmPages && value.wasmPages > this.buffer.byteLength / 65536) bad("BAD_WASM_PAGES");
        return value;
      }
    }
    bad("UNSTABLE_CONTROL");
  }
  transaction(fn, expectedEpoch = 0) {
    this.refresh();
    const latest = this.latest();
    if (!latest.epoch || latest.slot >= SNAPSHOT.SLOTS || (expectedEpoch && latest.epoch !== expectedEpoch)) return null;
    let control = this.control;
    const base = 16 + latest.slot * 16;
    if (Atomics.compareExchange(control, base, SNAPSHOT.READY, SNAPSHOT.READING) !== SNAPSHOT.READY) return null;
    try {
      // memory.grow replaces memory.buffer even after the slot has been pinned.
      this.refresh(); control = this.control;
      const slot = Array.from({length: 16}, (_, i) => Atomics.load(control, base + i) >>> 0);
      if (slot[0] !== SNAPSHOT.READING || slot[1] !== latest.epoch || slot[2] !== latest.layoutEpoch || slot[5] !== latest.revisionLo || slot[6] !== latest.revisionHi || slot[9] !== SNAPSHOT.SCHEMA || slot[10] !== 64) return null;
      if (slot.slice(11).some(Boolean)) bad("BAD_SLOT_RESERVED");
      const ptr = slot[3], bytes = slot[4];
      if (ptr % 16 || bytes < 512 || bytes % 16 || add(ptr, bytes) > this.buffer.byteLength) bad("BAD_SLOT");
      const u32 = new Uint32Array(this.buffer, ptr, bytes / 4);
      if (u32[0] !== SNAPSHOT.BLOB_MAGIC || u32[1] !== SNAPSHOT.SCHEMA || u32[2] !== 64 || u32[3] !== bytes || u32[4] !== slot[1] || u32[5] !== slot[5] || u32[6] !== slot[6] || u32[7] !== 14 || u32[8] !== 64 || u32[9] !== 32 || u32[10] !== slot[7] || u32[11] !== slot[8] || u32[12] !== 0x01020304 || u32[13] !== 3) bad("BAD_BLOB");
      if (u32[14] || u32[15]) bad("BAD_BLOB_RESERVED");
      const ranges = [], streams = {};
      for (let i = 0; i < 14; i++) {
        const d = 16 + i * 8, semantic = u32[d], scalar = u32[d + 1], offset = u32[d + 2], count = u32[d + 3], components = u32[d + 4], stride = u32[d + 5], width = u32[d + 6], reserved = u32[d + 7];
        const want = i < 5 ? slot[7] : slot[8];
        if (semantic !== i + 1 || scalar !== SCALARS[i] || count !== want || components !== COMPONENTS[i] || stride !== STRIDES[i] || width !== 4 || reserved || offset < 512 || offset % 16) bad("BAD_DESCRIPTOR");
        const end = add(offset, mul(stride, count));
        if (end > bytes) bad("BAD_DESCRIPTOR_RANGE");
        if (count) ranges.push([offset, end]);
        const Type = scalar === 2 ? Float32Array : Uint32Array;
        streams[STREAM_NAMES[i]] = new Type(this.buffer, add(ptr, offset), mul(count, components));
      }
      ranges.sort((a, b) => a[0] - b[0]);
      for (let i = 1; i < ranges.length; i++) if (ranges[i][0] < ranges[i - 1][1]) bad("OVERLAPPING_STREAMS");
      return fn(Object.freeze({epoch: slot[1], revisionLo: slot[5], revisionHi: slot[6], meshCount: slot[7], instanceCount: slot[8], streams: Object.freeze(streams)}));
    } finally {
      Atomics.store(control, base, SNAPSHOT.FREE); Atomics.notify(control, base);
    }
  }
}
