const WORDS = 24;
const MAX_SOURCE_BYTES = 384 * 1024 * 1024;
const CMD = { begin: 1, end: 2, clone: 3, destroy: 4, transform: 5, visible: 6, pipeline: 7, scene: 8 };

export class Mesh {
  constructor(renderer, slot, generation) {
    this.renderer = renderer;
    this.slot = slot >>> 0;
    this.generation = generation >>> 0;
    this.destroyed = false;
  }
  get transform() { return this.#snapshot().transform; }
  get visible() { return this.#snapshot().visible; }
  get shouldRender() { return this.#snapshot().shouldRender; }
  get pipelineKey() { return this.#snapshot().pipelineKey; }
  get geometry() { return this.#snapshot().geometry; }
  get geometryIdentity() { return this.#snapshot().geometry; }
  get geometrySlot() { return this.#snapshot().geometry.slot; }
  get geometryGeneration() { return this.#snapshot().geometry.generation; }
  get alive() { return this.#snapshot().alive; }
  get valid() { return this.#snapshot().valid; }
  clone() { this.#live(); return this.renderer._barrier(CMD.clone, this); }
  destroy() {
    this.#live();
    const result = this.renderer._barrier(CMD.destroy, this);
    if (result.admitted) this.destroyed = true;
    return result;
  }
  set visible(value) { this.#live(); this.renderer._assign(CMD.visible, this, [value ? 1 : 0]); }
  set transform(value) {
    this.#live();
    if (!value || value.length !== 16) throw new TypeError("transform must contain 16 numbers");
    this.renderer._assign(CMD.transform, this, Array.from(value, (v) => this.renderer._floatBits(v)));
  }
  set pipelineKey(value) { this.#live(); this.renderer._assign(CMD.pipeline, this, [value >>> 0]); }
  #snapshot() { this.#live(); return this.renderer._meshSnapshot(this.slot, this.generation); }
  #live() { if (this.destroyed) throw new Error("mesh handle is destroyed"); }
}

export class Renderer {
  constructor(memory) {
    if (!(memory?.buffer instanceof SharedArrayBuffer)) throw new Error("renderer requires shared WebAssembly.Memory");
    this.memory = memory;
    this.ready = false;
    this.sequence = 1;
    this.batch = 1;
    this.assignments = new Map();
    this.journal = [];
    this.pending = new Map();
    this.float = new Float32Array(1);
    this.bits = new Uint32Array(this.float.buffer);
    this.loadId = 1;
    this.failed = null;
    this.rendererWorker = null;
    this.latestIoRequest = null;
    this.ioRetries = 0;
    globalThis.rendererStartupFailed = (error) => this._fail(error instanceof Error ? error : new Error(`renderer startup failed: ${String(error)}`));
    globalThis.rendererWorkerReady = (worker) => {
      this.rendererWorker = worker;
      this._connectIoIfReady();
      worker.addEventListener("message", ({ data }) => {
        if (data?.type === "renderer-startup-error") this._fail(new Error(`renderer startup failed: ${data.message}`));
        if (data?.type === "renderer-progress") { this._pump(); this.commit(); }
      });
      worker.addEventListener("error", (event) => this._fail(new Error(`renderer worker failed: ${event.message || "unknown error"}`)));
      worker.addEventListener("messageerror", () => this._fail(new Error("renderer worker sent an unreadable message")));
    };
  }
  _connectIoIfReady() {
    if (!this.ready || !this.rendererWorker) return;
    if (!this.io) this._createIoWorker();
    const channel = new MessageChannel();
    this.rendererWorker.postMessage({ type: "io-port" }, [channel.port1]);
    this.io.postMessage({ type: "connect" }, [channel.port2]);
    if (this.latestIoRequest) this.io.postMessage(this.latestIoRequest);
  }
  _createIoWorker() {
    this.io = new Worker(new URL("./file-io-worker.js", import.meta.url), { type: "module" });
    const worker = this.io;
    const recover = (message) => {
      if (this.failed || this.io !== worker) return;
      console.error(message);
      this.io.terminate();
      if (this.ioRetries >= 1) {
        this._fail(new Error(`${message}; retry failed`));
        return;
      }
      this.ioRetries++;
      this._createIoWorker();
      if (this.rendererWorker) {
        const channel = new MessageChannel();
        this.rendererWorker.postMessage({ type: "io-port" }, [channel.port1]);
        this.io.postMessage({ type: "connect" }, [channel.port2]);
      }
      if (this.latestIoRequest) this.io.postMessage(this.latestIoRequest);
    };
    this.io.addEventListener("error", (event) => recover(`file I/O worker failed: ${event.message || "unknown error"}`));
    this.io.addEventListener("messageerror", () => recover("file I/O worker sent an unreadable message"));
  }
  _fail(error) {
    if (this.failed) return;
    this.failed = error;
    this.ready = false;
    this.io?.terminate();
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    this.journal.length = 0;
    this.assignments.clear();
  }
  attach(descriptor) {
    if (this.failed) return;
    this.d = Uint32Array.from(descriptor);
    if (this.d.length < 10 || this.d[1] !== 1) {
      this._fail(new Error(`unsupported renderer ABI ${this.d[1] ?? "missing"}`));
      return;
    }
    this.ready = true;
    this._connectIoIfReady();
    // A pre-attach microtask may already have observed !ready and returned.
    // Reschedule explicitly so its journal entries (and barrier promises) run.
    this._schedule();
    this._pump();
  }
  mesh(slot, generation) { return new Mesh(this, slot, generation); }
  meshes() {
    return this._projectionSnapshot()
      .filter((record) => record.status !== 0)
      .map((record) => new Mesh(this, record.slot, record.generation));
  }
  create(source) {
    if (!(source instanceof Mesh)) return Promise.reject(new TypeError("create currently requires source geometry via a Mesh"));
    return source.clone();
  }
  loadScene(source) {
    if (this.failed) return Promise.reject(this.failed);
    let external = true;
    if (source === "procedural") external = false;
    else if (typeof source !== "string" && !(source instanceof File) &&
             (!source || typeof source.getFile !== "function")) {
      return Promise.reject(new TypeError("scene source must be procedural, a URL, File, or file handle"));
    }
    if (source instanceof File && source.size > MAX_SOURCE_BYTES) {
      return Promise.reject(new RangeError("scene exceeds source byte budget"));
    }
    try { this._admit(1); }
    catch (error) { return Promise.reject(error); }
    const loadId = this._nextId("loadId");
    const barrier = this._barrier(CMD.scene, null, [loadId, external ? 1 : 0], true);
    if (!external) { this.latestIoRequest = null; return barrier; }
    const message = { type: "load", loadId };
    if (typeof source === "string") message.url = source;
    else if (source instanceof File) {
      message.file = source;
    }
    else message.handle = source;
    this.latestIoRequest = message;
    if (this.ready && this.io) this.io.postMessage(message); // cloned, never transferred
    return barrier;
  }
  _floatBits(value) { this.float[0] = value; return this.bits[0]; }
  _projectionSnapshot() {
    if (!this.ready) throw new Error("renderer ABI is not attached");
    const base = this.d[0] >>> 2;
    const capacity = this.d[8] >>> 0;
    const recordBytes = this.d[9] >>> 0;
    if (recordBytes < 22 * 4 || recordBytes % 4) throw new Error("invalid renderer projection layout");

    // The worker uses projection_epoch as a sequence lock. Reacquire the
    // buffer and all views on every attempt because WebAssembly memory may grow.
    for (let attempt = 0; attempt < 100; attempt++) {
      const buffer = this.memory.buffer;
      const words = new Uint32Array(buffer);
      const before = Atomics.load(words, base + 5) >>> 0;
      if (before & 1) continue;
      const projection = (this.d[0] + this.d[7]) >>> 2;
      const stride = recordBytes >>> 2;
      const floats = new Float32Array(buffer);
      const records = [];
      for (let slot = 0; slot < capacity; slot++) {
        const at = projection + slot * stride;
        const status = words[at + 1] >>> 0;
        if (!status) continue;
        records.push({
          slot,
          generation: words[at] >>> 0,
          status,
          geometry: Object.freeze({ slot: words[at + 2] >>> 0, generation: words[at + 3] >>> 0 }),
          pipelineKey: words[at + 4] >>> 0,
          renderFlags: words[at + 5] >>> 0,
          transform: new Float32Array(floats.slice(at + 6, at + 22)),
        });
      }
      const after = Atomics.load(words, base + 5) >>> 0;
      if (buffer === this.memory.buffer && before === after && !(after & 1)) return records;
    }
    throw new Error("could not obtain a coherent renderer projection snapshot");
  }
  _meshSnapshot(slot, generation) {
    const record = this._projectionSnapshot().find((candidate) => candidate.slot === slot);
    if (!record || record.generation !== (generation >>> 0)) {
      throw new Error("mesh handle is stale, tombstoned, or replaced");
    }
    const visible = (record.renderFlags & 1) !== 0;
    return { ...record, visible, shouldRender: visible, alive: true, valid: true };
  }
  _assign(op, mesh, payload) {
    const key = `${mesh.slot}:${mesh.generation}:${op}`;
    if (!this.assignments.has(key)) this._admit(1);
    this.assignments.set(key, [op, 0, mesh.slot, mesh.generation, ...payload]);
    this._schedule();
  }
  _barrier(op, mesh, payload = [], alreadyAdmitted = false) {
    if (this.failed) return Promise.reject(this.failed);
    if (!alreadyAdmitted) this._admit(1);
    this._flushAssignments();
    const request = this._nextId("sequence");
    this.journal.push([op, request, mesh?.slot ?? 0, mesh?.generation ?? 0, ...payload]);
    this._schedule();
    const loadId = op === CMD.scene ? payload[0] : null;
    const promise = new Promise((resolve, reject) => this.pending.set(request, { resolve, reject, op, loadId }));
    promise.admitted = true;
    return promise;
  }
  _admit(additional) {
    let maximum = 254;
    if (this.ready) {
      const words = new Int32Array(this.memory.buffer);
      const ring = (this.d[0] + this.d[3]) >>> 2;
      const capacity = words[ring + 2] >>> 0;
      if (capacity < 2 || words[ring + 3] >>> 0 !== WORDS) {
        const error = new Error("invalid renderer command ring header");
        this._fail(error);
        throw error;
      }
      maximum = capacity - 2;
    }
    if (this.journal.length + this.assignments.size + additional > maximum) {
      throw new RangeError("operation exceeds command journal capacity");
    }
  }
  _flushAssignments() { this.journal.push(...this.assignments.values()); this.assignments.clear(); }
  _nextId(field) {
    const value = this[field];
    if (!Number.isSafeInteger(value) || value <= 0 || value > 0xffffffff) {
      const error = new RangeError(`${field} identity space exhausted`);
      this._fail(error);
      throw error;
    }
    this[field] = value + 1;
    return value >>> 0;
  }
  _schedule() {
    if (this.scheduled) return;
    this.scheduled = true;
    queueMicrotask(() => { this.scheduled = false; this.commit(); });
  }
  commit() {
    if (!this.ready || (!this.journal.length && !this.assignments.size)) return false;
    this._flushAssignments();
    const base = this.d[0] >>> 2;
    const words = new Int32Array(this.memory.buffer);
    // One frame credit. Main never blocks and retains the complete journal on failure.
    if (Atomics.compareExchange(words, base + 4, 1, 0) !== 1) { this.rendererWorker?.postMessage({ type: "renderer-wake" }); return false; }
    const ring = (this.d[0] + this.d[3]) >>> 2;
    const capacity = words[ring + 2] >>> 0;
    const recordWords = words[ring + 3] >>> 0;
    if (capacity < 2 || recordWords !== WORDS) { this._fail(new Error("invalid renderer command ring header")); return false; }
    const head = Atomics.load(words, ring) >>> 0;
    const tail = Atomics.load(words, ring + 1) >>> 0;
    if (this.journal.length > capacity - 2) {
      Atomics.store(words, base + 4, 1);
      this._fail(new Error("sealed journal exceeds command ring capacity invariant"));
      return false;
    }
    const count = this.journal.length;
    const needed = count + 2;
    if ((head - tail) >>> 0 > capacity - needed) {
      Atomics.store(words, base + 4, 1);
      this.rendererWorker?.postMessage({ type: "renderer-wake" });
      return false;
    }
    const records = (this.d[0] + this.d[4]) >>> 2;
    const id = this._nextId("batch");
    const batch = this.journal.slice(0, count);
    const framed = [[CMD.begin, count, id], ...batch, [CMD.end, id]];
    framed.forEach((record, index) => {
      const at = records + (((head + index) >>> 0) % capacity) * WORDS;
      words.fill(0, at, at + WORDS);
      record.forEach((value, i) => { words[at + i] = value; });
    });
    this.journal.splice(0, count);
    Atomics.store(words, ring, (head + needed) >>> 0); // publish whole batch
    this.rendererWorker?.postMessage({ type: "renderer-wake" });
    return true;
  }
  _pump() {
    if (!this.ready) return;
    const words = new Int32Array(this.memory.buffer);
    const ring = (this.d[0] + this.d[5]) >>> 2;
    const capacity = words[ring + 2] >>> 0;
    if (!capacity || words[ring + 3] >>> 0 !== WORDS) { this._fail(new Error("invalid renderer completion ring header")); return; }
    const records = (this.d[0] + this.d[6]) >>> 2;
    let tail = Atomics.load(words, ring + 1) >>> 0;
    const head = Atomics.load(words, ring) >>> 0;
    while (tail !== head) {
      const at = records + ((tail >>> 0) % capacity) * WORDS;
      const request = words[at] >>> 0, status = words[at + 1] >>> 0;
      const pending = this.pending.get(request);
      if (pending) {
        this.pending.delete(request);
        if (pending.op === CMD.scene && this.latestIoRequest?.loadId === pending.loadId) {
          this.latestIoRequest = null;
        }
        if (status) pending.reject(new Error(status === 2 ? "scene load failed; existing scene retained" : "renderer rejected command or stale handle"));
        else if (pending.op === CMD.clone) pending.resolve(new Mesh(this, words[at + 2], words[at + 3]));
        else pending.resolve(pending.op === CMD.scene ? this.meshes() : undefined);
      }
      tail = (tail + 1) >>> 0;
    }
    Atomics.store(words, ring + 1, tail);
  }
}
