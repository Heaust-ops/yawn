const HEADER_WORDS = 16, SLOT_WORDS = 40, CAPACITY = 1024, SLOT_VERSION = 2;
const OP = { INSTALL_RENDER_DATA: 1, CREATE_INSTANCE: 3, DESTROY_INSTANCE: 6, COMPILE_GRAPH: 7, DROP_GRAPH: 8, SWITCH_GRAPH: 9, ALLOCATE_SOA: 11 };

export class RendererError extends Error {
  constructor(code, details) { super(details?.message ?? code); this.name = "RendererError"; this.code = code; this.details = details; }
}

export class YawnCore extends EventTarget {
  #bridge; #worker; #header; #slots; #buffer; #next = 1; #payload = 1;
  #pending = new Map(); #payloadPending = new Map(); #payloadActive = new Set(); #ready; #disposed = false;
  #readyResolve; #readyReject; #arrays = new Map();
  #transportReady = false;
  #telemetry; #stopped = false; #renderDataSnapshot;
  #graphQueue = []; #graphBusy = false;

  constructor(bridge) {
    super();
    this.#bridge = bridge;
    this.#worker = bridge.worker;
    this.#ready = new Promise((resolve, reject) => {
      this.#readyResolve = resolve;
      this.#readyReject = reject;
    });
    if (bridge.memory && Number.isInteger(bridge.ringPtr)) {
      this.#installTransport(bridge.memory, bridge.ringPtr);
    }
    this.#worker.addEventListener("message", e => this.#message(e.data));
    this.#worker.addEventListener("error", () => this.#fail("WORKER_ERROR"));
    this.#worker.addEventListener("messageerror", () => this.#fail("WORKER_MESSAGE_ERROR"));
    this.#worker.start?.();
  }

  get ready() { return this.#ready; }
  get telemetry() { return this.#telemetry; }
  get renderDataSnapshot() { return this.#renderDataSnapshot; }

  array(name) {
    const array = this.#arrays.get(name);
    if (!array) throw new RendererError("SOA_ARRAY_UNKNOWN", { message: `Unknown shared array '${name}'` });
    return array;
  }

  async allocateArray(layout) {
    await this.#ready;
    let source;
    try { source = JSON.stringify(layout); }
    catch (error) { throw new RendererError("SOA_LAYOUT_INVALID", { message: error?.message }); }
    const descriptor = await this.#withPayload(new TextEncoder().encode(source).buffer, OP.ALLOCATE_SOA);
    return this.#installArray(descriptor);
  }

  #refreshViews() {
    if (!this.#bridge.memory || !Number.isInteger(this.#bridge.ringPtr)) return;
    const buffer = this.#bridge.memory.buffer;
    if (buffer === this.#buffer) return;
    this.#buffer = buffer;
    this.#header = new Int32Array(buffer, this.#bridge.ringPtr, HEADER_WORDS);
    this.#slots = new Int32Array(buffer, this.#bridge.ringPtr + 64, CAPACITY * SLOT_WORDS);
  }

  #installTransport(memory, ringPtr) {
    this.#bridge.memory = memory;
    this.#bridge.ringPtr = ringPtr;
    this.#refreshViews();
    if (Atomics.load(this.#header, 0) !== 0x4e574159 || Atomics.load(this.#header, 1) !== 2 || Atomics.load(this.#header, 2) !== CAPACITY || Atomics.load(this.#header, 3) !== SLOT_WORDS) {
      const actual = Array.from(this.#header.subarray(0, 4), value => value >>> 0);
      try { this.#worker?.terminate?.(); } catch { /* best effort */ }
      try { this.#bridge?.free?.(); } catch { /* best effort */ }
      throw new RendererError("PROTOCOL_MISMATCH", { message: `Invalid command ring at ${ringPtr}: ${actual.join(",")}` });
    }
    this.#transportReady = true;
  }

  #message(message) {
    if (message?.type === "bootstrap") {
      if (this.#transportReady) { this.#fail("PROTOCOL_MISMATCH"); return; }
      try { this.#installTransport(message.memory, message.ringPtr); }
      catch (error) { this.#readyReject?.(error); this.#fail(error.code || "PROTOCOL_MISMATCH"); }
    } else if (message?.type === "reply") {
      const pending = this.#pending.get(message.request);
      if (!pending) return;
      this.#pending.delete(message.request);
      message.ok ? pending.resolve(message.result) : pending.reject(new RendererError(message.code, message.details));
    } else if (message?.type === "payload-ready") {
      const pending = this.#payloadPending.get(message.id);
      if (pending) { this.#payloadPending.delete(message.id); pending.resolve(); }
    } else if (message?.type === "telemetry") {
      this.#telemetry = message;
      this.dispatchEvent(new CustomEvent("renderer-frame", { detail: message }));
    } else if (message?.type === "fatal") {
      console.error("renderer worker fatal", JSON.stringify(message));
      this.#fail(message.code || "WORKER_FATAL");
    } else if (message?.type === "soa-init" || message?.type === "soa-layout") {
      try {
        for (const descriptor of message.arrays ?? []) this.#installArray(descriptor);
        if (message.type === "soa-init") this.#readyResolve?.(this);
        this.dispatchEvent(new CustomEvent("yawn-soa-layout", { detail: this.#arrays }));
      } catch (error) {
        this.#readyReject?.(error);
        this.#fail("SOA_PROTOCOL_MISMATCH");
      }
    } else if (message?.type === "snapshot-init") {
      try {
        if (message.controlVersion !== 1 || message.schemaVersion !== 2) throw new Error("version");
        this.#renderDataSnapshot = Object.freeze({
          memory: this.#bridge.memory,
          controlPtr: message.controlPtr,
          controlVersion: message.controlVersion,
          schemaVersion: message.schemaVersion,
        });
        this.dispatchEvent(new CustomEvent("yawn-render-data-snapshot", { detail: this.#renderDataSnapshot }));
      } catch { this.#fail("SNAPSHOT_PROTOCOL_MISMATCH"); }
    } else if (message?.type === "snapshot-published") {
      this.dispatchEvent(new CustomEvent("yawn-render-data-snapshot-published", {
        detail: Object.freeze({ epoch: message.epoch >>> 0 }),
      }));
    }
  }

  #installArray(descriptor) {
    const existing = this.#arrays.get(descriptor?.name);
    if (existing) existing.update(descriptor);
    else this.#arrays.set(descriptor?.name, new SharedSoaArray(this.#bridge.memory, descriptor));
    return this.#arrays.get(descriptor.name);
  }

  #stop() {
    if (this.#stopped) return;
    this.#stopped = true;
    try { this.#worker?.terminate?.(); } catch { /* best effort */ }
    try { this.#bridge?.free?.(); } catch { /* best effort */ }
    this.#bridge = null;
  }

  #fail(code) {
    if (this.#disposed) { this.#stop(); return; }
    this.#disposed = true;
    const error = new RendererError(code);
    this.#readyReject?.(error);
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
    for (const pending of this.#payloadPending.values()) pending.reject(error);
    this.#payloadPending.clear();
    for (const pending of this.#graphQueue) pending.reject(error);
    this.#graphQueue.length = 0;
    this.#stop();
  }

  #corrupt(code) {
    Atomics.store(this.#header, 6, 1);
    this.#fail(code);
    return Promise.reject(new RendererError(code));
  }

  #enqueue(opcode, words = []) {
    if (this.#disposed) return Promise.reject(new RendererError("DISPOSED"));
    this.#refreshViews();
    if (Atomics.load(this.#header, 6) !== 0) return this.#corrupt("RING_CLOSED");
    const read = Atomics.load(this.#header, 4) >>> 0;
    const write = Atomics.load(this.#header, 5) >>> 0;
    const backlog = (write - read) >>> 0;
    if (backlog > CAPACITY) return this.#corrupt("RING_CORRUPT");
    if (backlog === CAPACITY) return Promise.reject(new RendererError("RING_FULL"));
    let request = this.#next++ >>> 0;
    if (request === 0) { request = 1; this.#next = 2; }
    const base = (write % CAPACITY) * SLOT_WORDS;
    const promise = new Promise((resolve, reject) => this.#pending.set(request, { resolve, reject }));
    try {
      for (let i = 0; i < SLOT_WORDS; i++) Atomics.store(this.#slots, base + i, 0);
      for (let i = 0; i < words.length; i++) Atomics.store(this.#slots, base + 3 + i, words[i]);
      Atomics.store(this.#slots, base + 2, request);
      Atomics.store(this.#slots, base + 1, opcode);
      // The slot tag is its publication marker; write_index publishes the complete slot.
      Atomics.store(this.#slots, base, SLOT_VERSION);
      Atomics.store(this.#header, 5, (write + 1) | 0);
      Atomics.notify(this.#header, 5);
    } catch (error) {
      this.#pending.delete(request);
      this.#fail("PUBLICATION_FAILED");
      return Promise.reject(error);
    }
    return promise;
  }

  commitRenderDataUpload(array, byteLength) {
    if (this.#disposed) throw new RendererError("DISPOSED");
    if (!(array instanceof SharedSoaArray) || array.domain !== "fixed" || array.scalar !== "u32" || array.stride !== array.lanes * 4)
      throw new TypeError("array must be a packed fixed uint32 shared array");
    if (!Number.isInteger(byteLength) || byteLength < 1 || byteLength > array.length * array.lanes * 4)
      throw new RangeError("byteLength is outside the shared array");
    return this.#enqueue(OP.INSTALL_RENDER_DATA, [array.id, byteLength]);
  }

  async createInstance(mesh, transform, { type = Array(16).fill(0) } = {}) {
    validateHandle(mesh, "mesh");
    const result = await this.#enqueue(OP.CREATE_INSTANCE, [...mesh, ...floatWords(transform), ...typeWords(type)]);
    validateHandle(result, "instance");
    return result;
  }

  setInstanceTransform(instance, transform) {
    this.#validateLiveInstance(instance);
    this.array("instance.transform").write(instance[0], floatValues(transform), instance[1]);
  }

  setInstanceType(instance, type) {
    this.#validateLiveInstance(instance);
    this.array("instance.type").write(instance[0], typeWords(type), instance[1]);
  }

  destroyInstance(instance) {
    this.#validateLiveInstance(instance);
    return this.#enqueue(OP.DESTROY_INSTANCE, instance);
  }

  #validateLiveInstance(instance) {
    validateHandle(instance, "instance");
    const generation = this.array("instance.generation").read(instance[0])[0];
    if (generation !== instance[1]) throw new RendererError("STALE_HANDLE");
  }


  async #withPayload(buffer, opcode, words = []) {
    if (this.#disposed) throw new RendererError("DISPOSED");
    let id;
    do { id = this.#payload++ >>> 0; if (!id) id = this.#payload++ >>> 0; }
    while (!id || this.#payloadActive.has(id));
    this.#payloadActive.add(id);
    const worker = this.#worker;
    const ready = new Promise((resolve, reject) => this.#payloadPending.set(id, { resolve, reject }));
    try {
      worker.postMessage({ type: "payload", id, buffer }, [buffer]);
      await ready;
      return await this.#enqueue(opcode, [id, ...words]);
    } finally {
      this.#payloadPending.delete(id);
      this.#payloadActive.delete(id);
      try { worker.postMessage({ type: "payload-release", id }); } catch { /* best effort after termination */ }
    }
  }

  #graphCall(operation) {
    const result = new Promise((resolve, reject) => this.#graphQueue.push({operation, resolve, reject}));
    this.#pumpGraphQueue();
    return result;
  }

  #pumpGraphQueue() {
    if (this.#graphBusy || !this.#graphQueue.length) return;
    const call = this.#graphQueue.shift();
    if (this.#disposed) { call.reject(new RendererError("DISPOSED")); this.#pumpGraphQueue(); return; }
    this.#graphBusy = true;
    let outcome;
    try { outcome = call.operation(); } catch (error) { outcome = Promise.reject(error); }
    Promise.resolve(outcome).then(call.resolve, call.reject).finally(() => { this.#graphBusy = false; this.#pumpGraphQueue(); });
  }

  compileGraph(graph) {
    return this.#graphCall(() => this.#compileGraph(graph));
  }

  async #compileGraph(graph) {
    if (this.#disposed) throw new RendererError("DISPOSED");
    if (typeof graph !== "string") throw new TypeError("graph must be a serialized render-graph AST");
    const source = graph;
    const buffer = new TextEncoder().encode(source).buffer;
    if (buffer.byteLength > 1024 * 1024) throw new RendererError("GRAPH_PAYLOAD_TOO_LARGE");
    return this.#withPayload(buffer, OP.COMPILE_GRAPH);
  }

  dropCompiledGraph(compiledId) {
    validateCompiledId(compiledId);
    return this.#graphCall(() => this.#enqueue(OP.DROP_GRAPH, compiledId));
  }

  switchCompiledGraph(compiledId) {
    validateCompiledId(compiledId);
    if (compiledId[0] === 0 && compiledId[1] === 0) throw new TypeError("compiledId must be nonzero");
    return this.#graphCall(() => this.#enqueue(OP.SWITCH_GRAPH, compiledId));
  }

  dispose() { this.#fail("DISPOSED"); }
}

function validateCompiledId(compiledId) {
  if (!Array.isArray(compiledId) || compiledId.length !== 2 || compiledId.some(word => !Number.isInteger(word) || word < 0 || word > 0xffffffff)) throw new TypeError("compiledId must contain exactly two uint32 values");
}

function validateHandle(handle, name) {
  if (!Array.isArray(handle) || handle.length !== 2 || handle.some(word => !Number.isInteger(word) || word < 0 || word > 0xffffffff)) throw new TypeError(`${name} handle must contain exactly two uint32 values`);
}

function typeWords(words) { if (!words || words.length !== 16 || [...words].some(x => !Number.isInteger(x) || x < 0 || x > 0xffffffff)) throw new TypeError("type must contain exactly 16 uint32 values"); return Array.from(words, x => x >>> 0); }

function floatWords(matrix) {
  return [...new Int32Array(new Float32Array(floatValues(matrix)).buffer)];
}

function floatValues(values) {
  if (!values || values.length !== 16 || [...values].some(value => typeof value !== "number" || !Number.isFinite(value))) throw new TypeError("transform must contain 16 finite numbers");
  return Array.from(values);
}

const SOA_MAGIC = 0x414f5359;
const SCALAR_TAG = { u32: 1, i32: 2, f32: 3 };

export class SharedSoaArray {
  #memory; #descriptor; #buffer; #control; #words;

  constructor(memory, descriptor) {
    this.#memory = memory;
    this.update(descriptor);
  }

  get name() { return this.#descriptor.name; }
  get id() { return this.#descriptor.id; }
  get domain() { return this.#descriptor.domain; }
  get scalar() { return this.#descriptor.scalar; }
  get lanes() { return this.#descriptor.lanes; }
  get stride() { return this.#descriptor.stride; }
  get length() { this.#refresh(); return Atomics.load(this.#control, 6) >>> 0; }
  get capacity() { return this.#descriptor.capacity; }

  /** Returns the shared backing store and current wire descriptor for another worker. */
  share() {
    this.#refresh();
    return { buffer: this.#memory.buffer, descriptor: { ...this.#descriptor } };
  }

  update(descriptor) {
    if (!descriptor || typeof descriptor.name !== "string" || !SCALAR_TAG[descriptor.scalar] || typeof descriptor.writable !== "boolean" || (descriptor.generationGuard !== undefined && descriptor.generationGuard !== "instance" && descriptor.generationGuard !== "mesh"))
      throw new RendererError("SOA_PROTOCOL_MISMATCH");
    if (this.#descriptor && (descriptor.id !== this.#descriptor.id || descriptor.layoutEpoch < this.#descriptor.layoutEpoch))
      throw new RendererError("SOA_PROTOCOL_MISMATCH");
    this.#descriptor = Object.freeze({ ...descriptor });
    this.#buffer = null;
    this.#refresh();
  }

  #refresh() {
    const buffer = this.#memory.buffer;
    if (this.#buffer === buffer && this.#control?.byteOffset === this.#descriptor.controlPtr) return;
    const descriptor = this.#descriptor;
    if (!(buffer instanceof SharedArrayBuffer) || descriptor.controlPtr % 64 || descriptor.dataOffset !== 64 || descriptor.stride % 16)
      throw new RendererError("SOA_PROTOCOL_MISMATCH");
    this.#buffer = buffer;
    this.#control = new Int32Array(buffer, descriptor.controlPtr, 16);
    this.#words = new Int32Array(buffer, descriptor.controlPtr + descriptor.dataOffset, descriptor.byteLength / 4);
    if ((Atomics.load(this.#control, 0) >>> 0) !== SOA_MAGIC || (Atomics.load(this.#control, 1) >>> 0) !== 1 || (Atomics.load(this.#control, 2) >>> 0) !== descriptor.id || (Atomics.load(this.#control, 3) >>> 0) !== SCALAR_TAG[descriptor.scalar])
      throw new RendererError("SOA_PROTOCOL_MISMATCH");
  }

  #encode(value) {
    if (this.scalar === "u32") {
      if (!Number.isInteger(value) || value < 0 || value > 0xffffffff) throw new TypeError("value must be a uint32");
      return value | 0;
    }
    if (this.scalar === "i32") {
      if (!Number.isInteger(value) || value < -0x80000000 || value > 0x7fffffff) throw new TypeError("value must be an int32");
      return value | 0;
    }
    if (typeof value !== "number" || !Number.isFinite(value)) throw new TypeError("value must be a finite float32");
    return new Int32Array(new Float32Array([value]).buffer)[0];
  }

  #decode(word) {
    if (this.scalar === "u32") return word >>> 0;
    if (this.scalar === "i32") return word | 0;
    return new Float32Array(new Int32Array([word]).buffer)[0];
  }

  #lock() {
    this.#refresh();
    for (let attempt = 0; attempt < 1024; attempt++) {
      const sequence = Atomics.load(this.#control, 9) >>> 0;
      if (!(sequence & 1) && (Atomics.compareExchange(this.#control, 9, sequence | 0, (sequence + 1) | 0) >>> 0) === sequence)
        return sequence;
    }
    throw new RendererError("SOA_BUSY");
  }

  #unlock(sequence) {
    Atomics.store(this.#control, 9, (sequence + 2) | 0);
    Atomics.notify(this.#control, 9);
  }

  read(slot) {
    this.#refresh();
    if (!Number.isInteger(slot) || slot < 0 || slot >= this.length) throw new RangeError("slot is outside the shared array");
    const base = slot * (this.stride / 4);
    for (let attempt = 0; attempt < 1024; attempt++) {
      const before = Atomics.load(this.#control, 9) >>> 0;
      if (before & 1) continue;
      const values = Array.from({ length: this.lanes }, (_, lane) => this.#decode(Atomics.load(this.#words, base + lane)));
      const after = Atomics.load(this.#control, 9) >>> 0;
      if (before === after && !(after & 1)) return values;
    }
    throw new RendererError("SOA_BUSY");
  }

  write(slot, values, generation) {
    if (!this.#descriptor.writable) throw new RendererError("SOA_READ_ONLY");
    if (!values || values.length !== this.lanes) throw new TypeError(`values must contain ${this.lanes} lanes`);
    if (this.#descriptor.generationGuard !== undefined && (!Number.isInteger(generation) || generation < 1 || generation > 0xffffffff))
      throw new TypeError("generation must be a nonzero uint32");
    const encoded = Array.from(values, value => this.#encode(value));
    const sequence = this.#lock();
    try {
      if (!Number.isInteger(slot) || slot < 0 || slot >= (Atomics.load(this.#control, 6) >>> 0)) throw new RangeError("slot is outside the shared array");
      const base = slot * (this.stride / 4);
      encoded.forEach((word, lane) => Atomics.store(this.#words, base + lane, word));
      if (this.#descriptor.generationGuard !== undefined) {
        Atomics.store(this.#words, base + this.lanes, generation | 0);
        Atomics.add(this.#words, base + this.lanes + 1, 1);
      }
    } finally {
      this.#unlock(sequence);
    }
  }
}
