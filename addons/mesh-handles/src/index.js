import { RendererError } from "@yawn/core";
import { SnapshotReader } from "./snapshot.js";

const TOKEN = Symbol("yawn mesh handle addon");
const SNAPSHOT_EVENT = "yawn-render-data-snapshot";
const PUBLISHED_EVENT = "yawn-render-data-snapshot-published";
const createPickingWorker = () => new Worker(
  new URL("./bvh-worker.js", import.meta.url),
  { type: "module", name: "yawn-spatial-query" },
);

/** Conventional mesh/instance objects layered entirely over the Yawn core protocol. */
export class MeshHandles {
  #core;
  #factory; #worker; #reader; #snapshot; #epoch = 0; #next = 1; #picks = new Map(); #disposed = false;
  #onSnapshot; #onPublished;

  constructor(core, { pickingWorkerFactory = createPickingWorker } = {}) {
    this.#core = core;
    this.#factory = pickingWorkerFactory;
    this.#onSnapshot = event => this.#installSnapshot(event.detail);
    this.#onPublished = event => this.#publish(event.detail?.epoch);
    core.addEventListener?.(SNAPSHOT_EVENT, this.#onSnapshot);
    core.addEventListener?.(PUBLISHED_EVENT, this.#onPublished);
    if (core.renderDataSnapshot) this.#installSnapshot(core.renderDataSnapshot);
  }

  fromImportedScene(result) {
    if (!result || !Array.isArray(result.meshes)) throw new TypeError("invalid imported scene");
    return result.meshes.map((mesh) => new Mesh(TOKEN, this.#core, mesh));
  }

  async pickRay(origin, direction, options) {
    const result = await this.#pickRay(origin, direction, options);
    return {
      ...result,
      hits: result.hits.map((hit) => ({
        ...hit,
        instance: new Instance(TOKEN, this.#core, hit.instance),
      })),
    };
  }

  #installSnapshot(snapshot) {
    try {
      if (snapshot?.controlVersion !== 1 || snapshot?.schemaVersion !== 2) throw new Error("version");
      this.#snapshot = snapshot;
      this.#reader = new SnapshotReader(snapshot.memory, snapshot.controlPtr);
      this.#epoch = this.#reader.latest().epoch;
      this.#worker?.postMessage({ type: "init", ...snapshot });
    } catch {
      this.#disable("PICK_PROTOCOL_MISMATCH");
    }
  }

  #publish(epoch) {
    if (this.#disposed || !this.#reader) return;
    try {
      this.#epoch = this.#reader.latest().epoch;
      this.#worker?.postMessage({ type: "update", epoch: this.#epoch || (epoch >>> 0) });
    } catch {
      this.#disable("PICK_PROTOCOL_MISMATCH");
    }
  }

  #ensureWorker() {
    if (this.#worker) return true;
    if (!this.#reader || !this.#factory) return false;
    try {
      this.#worker = this.#factory();
      this.#worker.addEventListener("message", event => this.#workerMessage(event.data));
      this.#worker.addEventListener("error", () => this.#disable("PICK_WORKER_ERROR"));
      this.#worker.addEventListener("messageerror", () => this.#disable("PICK_WORKER_ERROR"));
      this.#worker.start?.();
      this.#worker.postMessage({ type: "init", ...this.#snapshot });
      if (this.#epoch) this.#worker.postMessage({ type: "update", epoch: this.#epoch });
      return true;
    } catch {
      this.#disable("PICK_WORKER_ERROR");
      return false;
    }
  }

  #disable(code) {
    const error = new RendererError(code);
    for (const pending of this.#picks.values()) pending.reject(error);
    this.#picks.clear();
    try { this.#worker?.terminate?.(); } catch { /* best effort */ }
    this.#worker = null;
  }

  #workerMessage(message) {
    if (message?.type === "fatal") { this.#disable(message.code || "PICK_WORKER_ERROR"); return; }
    if (message?.type !== "pick") return;
    const pending = this.#picks.get(message.request);
    if (!pending) return;
    this.#picks.delete(message.request);
    let latest;
    try { latest = this.#reader.latest().epoch; this.#epoch = latest; }
    catch { pending.reject(new RendererError("PICK_PROTOCOL_MISMATCH")); this.#disable("PICK_PROTOCOL_MISMATCH"); return; }
    if (message.stale || pending.epoch !== message.epoch || message.epoch !== latest) {
      if (!pending.retried && latest) this.#sendPick({ ...pending, retried: true }, latest);
      else pending.reject(new RendererError("PICK_STALE"));
      return;
    }
    pending.resolve({
      epoch: latest,
      hits: (message.hits || []).map(hit => ({
        instance: [hit.slot >>> 0, hit.generation >>> 0],
        distance: hit.distance,
      })),
    });
  }

  #sendPick(pending, epoch) {
    const request = this.#next++ >>> 0 || this.#next++;
    pending.epoch = epoch;
    this.#picks.set(request, pending);
    try {
      this.#worker.postMessage({
        type: "pick", request, epoch,
        origin: pending.origin, direction: pending.direction,
        maxDistance: pending.maxDistance, maxHits: pending.maxHits,
      });
    } catch {
      this.#picks.delete(request);
      pending.reject(new RendererError("PICK_WORKER_ERROR"));
    }
  }

  #pickRay(origin, direction, { maxDistance = Infinity, maxHits = 1 } = {}) {
    const vector = (value, name) => {
      if (!value || value.length !== 3 || [...value].some(x => typeof x !== "number" || !Number.isFinite(x)))
        throw new TypeError(`${name} must contain 3 finite numbers`);
      return [...value];
    };
    origin = vector(origin, "origin");
    direction = vector(direction, "direction");
    if (direction.every(x => x === 0)) throw new TypeError("direction must be nonzero");
    if (typeof maxDistance !== "number" || (!(Number.isFinite(maxDistance) && maxDistance >= 0) && maxDistance !== Infinity) || !Number.isInteger(maxHits) || maxHits < 1 || maxHits > 64)
      throw new TypeError("invalid pick options");
    if (this.#disposed) return Promise.reject(new RendererError("DISPOSED"));
    if (!this.#ensureWorker()) return Promise.reject(new RendererError("PICK_UNAVAILABLE"));
    let epoch;
    try { epoch = this.#reader.latest().epoch; this.#epoch = epoch; }
    catch { return Promise.reject(new RendererError("PICK_PROTOCOL_MISMATCH")); }
    if (!epoch) return Promise.reject(new RendererError("PICK_STALE"));
    return new Promise((resolve, reject) => this.#sendPick({
      resolve, reject, origin, direction, maxDistance, maxHits, retried: false,
    }, epoch));
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#core.removeEventListener?.(SNAPSHOT_EVENT, this.#onSnapshot);
    this.#core.removeEventListener?.(PUBLISHED_EVENT, this.#onPublished);
    try { this.#worker?.postMessage?.({ type: "dispose" }); } catch { /* best effort */ }
    this.#disable("DISPOSED");
  }
}

export class Mesh {
  #core; #handle; #defaultInstance; #defaultType;

  constructor(token, core, descriptor) {
    if (token !== TOKEN) throw new TypeError("Mesh cannot be constructed directly");
    this.#core = core;
    this.#handle = Object.freeze([...descriptor.handle]);
    this.#defaultInstance = new Instance(TOKEN, core, descriptor.defaultInstance);
    this.#defaultType = Object.freeze([...descriptor.defaultType]);
  }

  get handle() { return this.#handle; }
  get defaultInstance() { return this.#defaultInstance; }

  async createInstance(transform, { type = this.#defaultType } = {}) {
    return new Instance(TOKEN, this.#core, await this.#core.createInstance(this.#handle, transform, { type }));
  }
}

export class Instance {
  #core; #handle; #dead = false;

  constructor(token, core, handle) {
    if (token !== TOKEN) throw new TypeError("Instance cannot be constructed directly");
    this.#core = core;
    this.#handle = Object.freeze([...handle]);
  }

  get handle() { return this.#handle; }
  #live() { if (this.#dead) throw new Error("STALE_HANDLE"); }
  setType(words) { this.#live(); this.#core.setInstanceType(this.#handle, words); }
  setTransform(transform) { this.#live(); this.#core.setInstanceTransform(this.#handle, transform); }
  async destroy() { this.#live(); await this.#core.destroyInstance(this.#handle); this.#dead = true; }
}

function requireArray(core, name, { domain, scalar, lanes }) {
  if (!core?.array) throw new TypeError("core must implement the Yawn shared render-data protocol");
  const array = core.array(name);
  if (array.domain !== domain || array.scalar !== scalar || array.lanes !== lanes)
    throw new RendererError("SOA_PROTOCOL_MISMATCH");
  return array;
}

function vector(value, length, name) {
  if (!value || value.length !== length || [...value].some(item => typeof item !== "number" || !Number.isFinite(item)))
    throw new TypeError(`${name} must contain ${length} finite numbers`);
  return Array.from(value);
}

function finite(value, name) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new TypeError(`${name} must be finite`);
  return value;
}

/** Conventional camera properties backed by the canonical SIMD-width camera SOA row. */
export class CameraHandle {
  #array;

  constructor(core) {
    this.#array = requireArray(core, "camera.state", { domain: "fixed", scalar: "f32", lanes: 16 });
  }

  get state() { return this.#array.read(0); }
  set state(value) { this.#write(vector(value, 16, "state")); }
  get position() { return this.state.slice(0, 3); }
  set position(value) { this.update({ position: value }); }
  get target() { return this.state.slice(4, 7); }
  set target(value) { this.update({ target: value }); }
  get up() { return this.state.slice(8, 11); }
  set up(value) { this.update({ up: value }); }
  get fovY() { return this.state[12]; }
  set fovY(value) { this.update({ fovY: value }); }
  get aspect() { return this.state[13]; }
  set aspect(value) { this.update({ aspect: value }); }
  get near() { return this.state[14]; }
  set near(value) { this.update({ near: value }); }
  get far() { return this.state[15]; }
  set far(value) { this.update({ far: value }); }

  update(properties = {}) {
    if (!properties || typeof properties !== "object") throw new TypeError("camera properties must be an object");
    const known = new Set(["position", "target", "up", "fovY", "aspect", "near", "far"]);
    for (const key of Object.keys(properties)) if (!known.has(key)) throw new TypeError(`unknown camera property '${key}'`);
    const state = this.state;
    if (properties.position !== undefined) state.splice(0, 3, ...vector(properties.position, 3, "position"));
    if (properties.target !== undefined) state.splice(4, 3, ...vector(properties.target, 3, "target"));
    if (properties.up !== undefined) state.splice(8, 3, ...vector(properties.up, 3, "up"));
    if (properties.fovY !== undefined) state[12] = finite(properties.fovY, "fovY");
    if (properties.aspect !== undefined) state[13] = finite(properties.aspect, "aspect");
    if (properties.near !== undefined) state[14] = finite(properties.near, "near");
    if (properties.far !== undefined) state[15] = finite(properties.far, "far");
    this.#write(state);
    return this;
  }

  lookAt(position, target, { up = this.up } = {}) {
    return this.update({ position, target, up });
  }

  #write(state) {
    const offset = state.slice(0, 3).map((value, axis) => state[4 + axis] - value);
    const up = state.slice(8, 11);
    const cross = [
      offset[1] * up[2] - offset[2] * up[1],
      offset[2] * up[0] - offset[0] * up[2],
      offset[0] * up[1] - offset[1] * up[0],
    ];
    if (Math.hypot(...offset) < 0.1 || Math.hypot(...up) === 0 || Math.hypot(...cross) === 0)
      throw new RangeError("camera position, target, and up do not define a view");
    if (!(state[12] > 0 && state[12] < Math.PI) || state[13] <= 0 || state[14] <= 0 || state[15] <= state[14])
      throw new RangeError("camera projection is invalid");
    state[3] = state[7] = 1;
    state[11] = 0;
    this.#array.write(0, state);
  }
}

const FLOAT_WORD = new ArrayBuffer(4);
const FLOAT_VIEW = new Float32Array(FLOAT_WORD);
const WORD_VIEW = new Uint32Array(FLOAT_WORD);
function wordToFloat(word) { WORD_VIEW[0] = word; return FLOAT_VIEW[0]; }
function floatToWord(value) { FLOAT_VIEW[0] = value; return WORD_VIEW[0]; }

/** Creates scene-scoped material objects over packed material SOA rows. */
export class MaterialHandles {
  #array;

  constructor(core) {
    this.#array = requireArray(core, "material.state", { domain: "fixed", scalar: "u32", lanes: 28 });
  }

  fromImportedScene(result) {
    if (!result || !Array.isArray(result.materials)) throw new TypeError("invalid imported scene");
    return result.materials.map(material => this.get(material?.key));
  }

  get(key) {
    if (!Number.isInteger(key) || key < 0 || key > 0xffffffff || key >= this.#array.length)
      throw new RangeError("material key is outside the shared material rows");
    return new MaterialHandle(TOKEN, this.#array, key);
  }
}

export class MaterialHandle {
  #array; #key;

  constructor(token, array, key) {
    if (token !== TOKEN) throw new TypeError("MaterialHandle cannot be constructed directly");
    this.#array = array;
    this.#key = key;
  }

  get key() { return this.#key; }
  get baseColor() { return this.#floats(0, 4); }
  set baseColor(value) { this.update({ baseColor: value }); }
  get emissive() { return this.#floats(4, 3); }
  set emissive(value) { this.update({ emissive: value }); }
  get metallic() { return this.#float(8); }
  set metallic(value) { this.update({ metallic: value }); }
  get roughness() { return this.#float(9); }
  set roughness(value) { this.update({ roughness: value }); }
  get normalScale() { return this.#float(10); }
  set normalScale(value) { this.update({ normalScale: value }); }
  get occlusionStrength() { return this.#float(11); }
  set occlusionStrength(value) { this.update({ occlusionStrength: value }); }
  get alphaCutoff() { return this.#float(13); }
  set alphaCutoff(value) { this.update({ alphaCutoff: value }); }
  get ior() { return this.#float(14); }
  set ior(value) { this.update({ ior: value }); }

  update(properties = {}) {
    if (!properties || typeof properties !== "object") throw new TypeError("material properties must be an object");
    const known = new Set(["baseColor", "emissive", "metallic", "roughness", "normalScale", "occlusionStrength", "alphaCutoff", "ior"]);
    for (const key of Object.keys(properties)) if (!known.has(key)) throw new TypeError(`unknown material property '${key}'`);
    const words = this.#array.read(this.#key);
    const setFloat = (lane, value, name) => { words[lane] = floatToWord(finite(value, name)); };
    if (properties.baseColor !== undefined)
      vector(properties.baseColor, 4, "baseColor").forEach((value, lane) => setFloat(lane, value, "baseColor"));
    if (properties.emissive !== undefined)
      vector(properties.emissive, 3, "emissive").forEach((value, lane) => setFloat(4 + lane, value, "emissive"));
    for (const [name, lane] of [["metallic", 8], ["roughness", 9], ["normalScale", 10], ["occlusionStrength", 11], ["alphaCutoff", 13]]) {
      if (properties[name] !== undefined) setFloat(lane, properties[name], name);
    }
    if (properties.metallic !== undefined && !(properties.metallic >= 0 && properties.metallic <= 1)) throw new RangeError("metallic must be in [0, 1]");
    if (properties.roughness !== undefined && !(properties.roughness >= 0 && properties.roughness <= 1)) throw new RangeError("roughness must be in [0, 1]");
    if (properties.occlusionStrength !== undefined && !(properties.occlusionStrength >= 0 && properties.occlusionStrength <= 1)) throw new RangeError("occlusionStrength must be in [0, 1]");
    if (properties.alphaCutoff !== undefined && !(properties.alphaCutoff >= 0 && properties.alphaCutoff <= 1)) throw new RangeError("alphaCutoff must be in [0, 1]");
    if (properties.ior !== undefined) {
      const ior = finite(properties.ior, "ior");
      if (ior !== 0 && ior < 1) throw new RangeError("ior must be 0 or at least 1");
      setFloat(14, ior, "ior");
      setFloat(15, ior === 0 ? 1 : ((ior - 1) / (ior + 1)) ** 2, "ior");
    }
    this.#array.write(this.#key, words);
    return this;
  }

  #float(lane) { return wordToFloat(this.#array.read(this.#key)[lane]); }
  #floats(start, length) { return this.#array.read(this.#key).slice(start, start + length).map(wordToFloat); }
}
