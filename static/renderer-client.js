import { SnapshotReader } from "./render-data-snapshot.js";

export const VISIBLE = 1;
const HEADER_WORDS = 16, SLOT_WORDS = 24, CAPACITY = 1024, SLOT_VERSION = 1;
const OP = { IMPORT_GLB: 1, MESH_FLAGS: 2, CREATE_INSTANCE: 3, INSTANCE_FLAGS: 4, INSTANCE_TRANSFORM: 5, DESTROY_INSTANCE: 6, COMPILE_GRAPH: 7, DROP_GRAPH: 8, SWITCH_GRAPH: 9 };
const HANDLE_TOKEN = Symbol("renderer handle");

export class RendererError extends Error {
  constructor(code, details) { super(details?.message ?? code); this.name = "RendererError"; this.code = code; this.details = details; }
}

export class RendererClient {
  #bridge; #worker; #header; #slots; #buffer; #next = 1; #payload = 1;
  #pending = new Map(); #payloadPending = new Map(); #payloadActive = new Set(); #ready; #disposed = false;
  #telemetry; #profile; #stopped = false;
  #graphQueue = []; #graphBusy = false;
  #bvh; #snapshotReader; #picking = true; #snapshotEpoch = 0; #pickNext = 1; #picks = new Map();

  constructor(bridge) {
    this.#bridge = bridge;
    this.#worker = bridge.worker;
    this.#refreshViews();
    if (Atomics.load(this.#header, 0) !== 0x4e574159 || Atomics.load(this.#header, 1) !== 1 || Atomics.load(this.#header, 2) !== CAPACITY || Atomics.load(this.#header, 3) !== SLOT_WORDS) {
      try { this.#worker?.terminate?.(); } catch { /* best effort */ }
      try { bridge?.free?.(); } catch { /* best effort */ }
      throw new RendererError("PROTOCOL_MISMATCH");
    }
    this.#worker.addEventListener("message", e => this.#message(e.data));
    this.#worker.addEventListener("error", () => this.#fail("WORKER_ERROR"));
    this.#worker.addEventListener("messageerror", () => this.#fail("WORKER_MESSAGE_ERROR"));
    try {
      const factory = bridge.workerFactory || (() => new Worker(new URL("./bvh-worker.js", import.meta.url), { type: "module" }));
      this.#bvh = factory();
      this.#bvh.addEventListener("message", e => this.#bvhMessage(e.data));
      this.#bvh.addEventListener("error", () => this.#disablePicking("PICKING_FAILED"));
      this.#bvh.addEventListener("messageerror", () => this.#disablePicking("PICKING_FAILED"));
    } catch { this.#picking = false; }
    this.#ready = Promise.resolve(this);
  }

  get ready() { return this.#ready; }
  get telemetry() { return this.#telemetry; }
  get profile() { return this.#profile; }

  #refreshViews() {
    const buffer = this.#bridge.memory.buffer;
    if (buffer === this.#buffer) return;
    this.#buffer = buffer;
    this.#header = new Int32Array(buffer, this.#bridge.ringPtr, HEADER_WORDS);
    this.#slots = new Int32Array(buffer, this.#bridge.ringPtr + 64, CAPACITY * SLOT_WORDS);
  }

  #message(message) {
    if (message?.type === "reply") {
      const pending = this.#pending.get(message.request);
      if (!pending) return;
      this.#pending.delete(message.request);
      message.ok ? pending.resolve(message.result) : pending.reject(new RendererError(message.code, message.details));
    } else if (message?.type === "payload-ready") {
      const pending = this.#payloadPending.get(message.id);
      if (pending) { this.#payloadPending.delete(message.id); pending.resolve(); }
    } else if (message?.type === "telemetry") {
      this.#telemetry = message;
      dispatchEvent(new CustomEvent("renderer-frame", { detail: message }));
    } else if (message?.type === "profile-snapshot") {
      this.#profile = message;
      dispatchEvent(new CustomEvent("renderer-profile", { detail: message }));
    } else if (message?.type === "fatal") {
      console.error("renderer worker fatal", message.code, message.message);
      this.#fail(message.code || "WORKER_FATAL");
    } else if (message?.type === "snapshot-init") {
      try {
        if (message.controlVersion !== 1 || message.schemaVersion !== 1) throw new Error("version");
        this.#snapshotReader = new SnapshotReader(this.#bridge.memory, message.controlPtr);
        this.#bvh?.postMessage({type:"init",memory:this.#bridge.memory,controlPtr:message.controlPtr,controlVersion:1,schemaVersion:1});
      } catch { this.#disablePicking("PICK_PROTOCOL_MISMATCH"); }
    } else if (message?.type === "snapshot-published") {
      try { this.#snapshotEpoch=this.#snapshotReader?.latest().epoch||0; this.#bvh?.postMessage({type:"update",epoch:this.#snapshotEpoch}); } catch { this.#disablePicking("PICK_PROTOCOL_MISMATCH"); }
    }
  }

  #disablePicking(code) { this.#picking=false; const allowed=new Set(["PICK_UNAVAILABLE","PICK_PROTOCOL_MISMATCH","PICK_WORKER_ERROR","PICK_STALE","DISPOSED"]); const error=new RendererError(allowed.has(code)?code:"PICK_WORKER_ERROR"); for(const p of this.#picks.values())p.reject(error); this.#picks.clear(); try{this.#bvh?.terminate?.();}catch{} this.#bvh=null; }
  #bvhMessage(message) {
    if(message?.type==="fatal"){this.#disablePicking(message.code);return;}
    if(message?.type!=="pick")return;
    const p=this.#picks.get(message.request);if(!p)return;this.#picks.delete(message.request);
    let latest=0;try{latest=this.#snapshotReader.latest().epoch;this.#snapshotEpoch=latest;}catch{this.#disablePicking("PICK_PROTOCOL_MISMATCH");p.reject(new RendererError("PICK_PROTOCOL_MISMATCH"));return;}
    if(message.stale||p.epoch!==message.epoch||message.epoch!==latest){if(!p.retried&&latest){this.#sendPick({...p,retried:true},latest);}else p.reject(new RendererError("PICK_STALE"));return;}
    const hits=(message.hits||[]).map(hit=>({instance:this.#instance([hit.slot>>>0,hit.generation>>>0]),distance:hit.distance}));p.resolve({epoch:latest,hits});
  }
  #sendPick(p,epoch){const request=this.#pickNext++>>>0||this.#pickNext++;p.epoch=epoch;this.#picks.set(request,p);try{this.#bvh.postMessage({type:"pick",request,epoch,origin:p.origin,direction:p.direction,maxDistance:p.maxDistance,maxHits:p.maxHits});}catch{this.#picks.delete(request);p.reject(new RendererError("PICK_WORKER_ERROR"));}}

  pickRay(origin,direction,{maxDistance=Infinity,maxHits=1}={}) {
    const vector=(v,name)=>{if(!v||v.length!==3||[...v].some(x=>typeof x!=="number"||!Number.isFinite(x)))throw new TypeError(`${name} must contain 3 finite numbers`);return [...v];};
    origin=vector(origin,"origin");direction=vector(direction,"direction");if(direction.every(x=>x===0))throw new TypeError("direction must be nonzero");if(typeof maxDistance!=="number"||(!(Number.isFinite(maxDistance)&&maxDistance>=0)&&maxDistance!==Infinity)||!Number.isInteger(maxHits)||maxHits<1||maxHits>64)throw new TypeError("invalid pick options");
    if(this.#disposed)return Promise.reject(new RendererError("DISPOSED"));if(!this.#picking||!this.#bvh||!this.#snapshotReader)return Promise.reject(new RendererError("PICK_UNAVAILABLE"));
    let epoch;try{epoch=this.#snapshotReader.latest().epoch;this.#snapshotEpoch=epoch;}catch{return Promise.reject(new RendererError("PICK_PROTOCOL_MISMATCH"));}if(!epoch)return Promise.reject(new RendererError("PICK_STALE"));
    return new Promise((resolve,reject)=>this.#sendPick({resolve,reject,origin,direction,maxDistance,maxHits,retried:false},epoch));
  }

  #stop() {
    if (this.#stopped) return;
    this.#stopped = true;
    try { this.#worker?.terminate?.(); } catch { /* best effort */ }
    try { this.#bvh?.postMessage?.({type:"dispose"}); this.#bvh?.terminate?.(); } catch { /* best effort */ }
    try { this.#bridge?.free?.(); } catch { /* best effort */ }
    this.#bridge = null;
  }

  #fail(code) {
    if (this.#disposed) { this.#stop(); return; }
    this.#disposed = true;
    const error = new RendererError(code);
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
    for (const pending of this.#payloadPending.values()) pending.reject(error);
    this.#payloadPending.clear();
    for (const pending of this.#graphQueue) pending.reject(error);
    this.#graphQueue.length = 0;
    this.#disablePicking(code === "DISPOSED" ? "DISPOSED" : "PICK_WORKER_ERROR");
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

  #mesh(handle) {
    return new Mesh(HANDLE_TOKEN,
      visible => this.#enqueue(OP.MESH_FLAGS, [...handle, visible ? VISIBLE : 0]),
      async (transform, visible) => {
        const result = await this.#enqueue(OP.CREATE_INSTANCE, [...handle, ...floatWords(transform), visible ? VISIBLE : 0]);
        return this.#instance(result);
      });
  }

  #instance(handle) {
    return new Instance(HANDLE_TOKEN,
      visible => this.#enqueue(OP.INSTANCE_FLAGS, [...handle, visible ? VISIBLE : 0]),
      transform => this.#enqueue(OP.INSTANCE_TRANSFORM, [...handle, ...floatWords(transform)]),
      () => this.#enqueue(OP.DESTROY_INSTANCE, [...handle]));
  }

  async replaceSceneGlb(source, { framing = "exterior" } = {}) {
    if (this.#disposed) throw new RendererError("DISPOSED");
    if (framing !== "exterior" && framing !== "interior") throw new TypeError("framing must be exterior or interior");
    let buffer;
    if (typeof source === "string" || source instanceof URL) buffer = await (await fetch(source)).arrayBuffer();
    else if (typeof File !== "undefined" && source instanceof File) buffer = await source.arrayBuffer();
    else if (source instanceof ArrayBuffer) buffer = source;
    else throw new TypeError("GLB source must be URL, File, or ArrayBuffer");
    if (this.#disposed) throw new RendererError("DISPOSED");
    const result = await this.#withPayload(buffer, OP.IMPORT_GLB, [framing === "interior" ? 1 : 0]);
    return result.meshes.map(handle => this.#mesh(handle));
  }

  /** Compatibility alias for the original opcode-1 API. */
  importGlb(source, options) { return this.replaceSceneGlb(source, options); }

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
    let json;
    try { json = JSON.stringify(graph); } catch (error) { throw new RendererError("GRAPH_JSON_INVALID", { message: error?.message || "GRAPH_JSON_INVALID" }); }
    if (json === undefined) throw new RendererError("GRAPH_JSON_INVALID");
    const buffer = new TextEncoder().encode(json).buffer;
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
    return this.#graphCall(() => this.#enqueue(OP.SWITCH_GRAPH, [1, ...compiledId]));
  }

  switchToImmediate() {
    return this.#graphCall(() => this.#enqueue(OP.SWITCH_GRAPH, [0, 0, 0]));
  }

  dispose() { this.#fail("DISPOSED"); }
}

function validateCompiledId(compiledId) {
  if (!Array.isArray(compiledId) || compiledId.length !== 2 || compiledId.some(word => !Number.isInteger(word) || word < 0 || word > 0xffffffff)) throw new TypeError("compiledId must contain exactly two uint32 values");
}

function floatWords(matrix) {
  if (!matrix || matrix.length !== 16) throw new TypeError("transform must contain 16 numbers");
  return [...new Int32Array(new Float32Array(matrix).buffer)];
}

class Mesh {
  #setVisible; #createInstance;
  constructor(token, setVisible, createInstance) {
    if (token !== HANDLE_TOKEN) throw new TypeError("Mesh cannot be constructed directly");
    this.#setVisible = setVisible;
    this.#createInstance = createInstance;
  }
  setVisible(visible) { return this.#setVisible(visible); }
  createInstance(transform, visible = true) { return this.#createInstance(transform, visible); }
}

class Instance {
  #setVisible; #setTransform; #destroy; #dead = false;
  constructor(token, setVisible, setTransform, destroy) {
    if (token !== HANDLE_TOKEN) throw new TypeError("Instance cannot be constructed directly");
    this.#setVisible = setVisible;
    this.#setTransform = setTransform;
    this.#destroy = destroy;
  }
  #live() { if (this.#dead) throw new RendererError("STALE_HANDLE"); }
  setVisible(visible) { this.#live(); return this.#setVisible(visible); }
  setTransform(transform) { this.#live(); return this.#setTransform(transform); }
  async destroy() { this.#live(); await this.#destroy(); this.#dead = true; }
}
