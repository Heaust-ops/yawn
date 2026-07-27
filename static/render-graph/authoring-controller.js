import { mapAuthoringDiagnostic } from "./adapter.js";

export class AuthoringController {
  #renderer; #adapt; #revision = 0; #nextRevision = 1; #generation = 0;
  #current; #lastGood; #applyPromise; #listeners = new Set();
  #owned = new Map(); #drops = new Map(); #activeCompiles = new Set();
  #disposed = false; #applyingRecord; #scheduler; #debounceMs; #timer; #destroyPromise;

  constructor({ renderer, adapt, scheduler = globalThis, debounceMs = 150 }) {
    this.#renderer = renderer; this.#adapt = adapt;
    this.#scheduler = scheduler; this.#debounceMs = debounceMs;
  }
  get revision() { return this.#revision; }
  get dirty() { return !!this.#current; }
  get applying() { return !!this.#applyPromise; }
  get staged() { return this.#current?.candidate ?? null; }
  get canApply() { return !this.#disposed && !!this.#current?.candidate && !this.#applyPromise; }
  subscribe(fn) {
    if (this.#disposed) return () => {};
    this.#listeners.add(fn); fn(this.#state());
    return () => this.#listeners.delete(fn);
  }
  #state() { return { revision: this.#revision, dirty: this.dirty, applying: this.applying, staged: this.staged, canApply: this.canApply, error: this.#current?.diagnostic ?? null }; }
  #emit() { if (!this.#disposed) for (const fn of this.#listeners) fn(this.#state()); }
  #key(id) { return JSON.stringify(id); }
  #drop(candidate) {
    if (!candidate) return Promise.resolve();
    const key = this.#key(candidate.compiledId);
    if (!this.#owned.has(key)) return this.#drops.get(key) ?? Promise.resolve();
    if (this.#drops.has(key)) return this.#drops.get(key);
    let result;
    try { result = this.#renderer.dropCompiledGraph(candidate.compiledId); }
    catch (error) { result = Promise.reject(error); }
    const dropping = Promise.resolve(result)
      .then(() => { this.#owned.delete(key); })
      .finally(() => { this.#drops.delete(key); });
    this.#drops.set(key, dropping);
    return dropping;
  }
  #retire(candidate) { if (candidate) void this.#drop(candidate).catch(() => {}); }
  #start(record) {
    if (!record.compile) {
      record.compile = this.#compile(record);
      this.#activeCompiles.add(record.compile);
      record.compile.finally(() => this.#activeCompiles.delete(record.compile));
    }
    return record.compile;
  }
  #flush(record = this.#current) {
    if (this.#timer) { this.#scheduler.clearTimeout(this.#timer); this.#timer = undefined; }
    return record ? this.#start(record) : null;
  }
  markDirty(snapshot) {
    if (this.#disposed) return;
    const previous = this.#current;
    const record = { generation: ++this.#generation, snapshot, candidate: null, error: null, diagnostic: null, compile: null };
    this.#current = record;
    if (previous?.candidate && previous !== this.#applyingRecord && previous.candidate !== this.#lastGood) this.#retire(previous.candidate);
    if (this.#timer) this.#scheduler.clearTimeout(this.#timer);
    this.#timer = this.#scheduler.setTimeout(() => { this.#timer = undefined; if (!this.#disposed) this.#start(record); }, this.#debounceMs);
    this.#emit();
  }
  async #compile(record) {
    let candidate, ir;
    try {
      ir = this.#adapt(record.snapshot, this.#nextRevision++);
      candidate = await this.#renderer.compileGraph(ir);
      this.#owned.set(this.#key(candidate.compiledId), candidate);
      if (this.#disposed || this.#current !== record) { this.#retire(candidate); return null; }
      record.candidate = candidate; record.error = record.diagnostic = null; this.#emit(); return candidate;
    } catch (error) {
      if (candidate) this.#retire(candidate);
      record.error = error;
      record.diagnostic = ir ? mapAuthoringDiagnostic(ir, error) : error;
      if (this.#current === record) this.#emit();
      return null;
    }
  }
  apply() {
    if (this.#disposed) return Promise.resolve(null);
    if (this.#applyPromise) return this.#applyPromise;
    const record = this.#current;
    if (!record) return Promise.resolve(this.#lastGood);
    this.#applyingRecord = record; this.#flush(record);
    this.#applyPromise = this.#applyRecord(record); this.#emit(); return this.#applyPromise;
  }
  async #applyRecord(record) {
    try {
      const candidate = record.candidate ?? (await record.compile);
      if (!candidate) throw record.error ?? new Error("Graph compilation failed");
      if (this.#disposed || this.#current !== record) { this.#retire(candidate); return null; }
      await this.#renderer.switchCompiledGraph(candidate.compiledId);
      const old = this.#lastGood; this.#lastGood = candidate;
      this.#revision = candidate.revision ?? this.#revision + 1;
      if (this.#current === record) this.#current = undefined;
      if (old && old !== candidate) this.#retire(old);
      return candidate;
    } finally {
      if (this.#current !== record && record.candidate && record.candidate !== this.#lastGood) this.#retire(record.candidate);
      this.#applyPromise = null; this.#applyingRecord = undefined; this.#emit();
    }
  }
  destroy() {
    if (this.#destroyPromise) return this.#destroyPromise;
    this.#disposed = true;
    if (this.#timer) { this.#scheduler.clearTimeout(this.#timer); this.#timer = undefined; }
    this.#listeners.clear();
    this.#destroyPromise = this.#finish();
    return this.#destroyPromise;
  }
  async #finish() {
    const applying = this.#applyPromise;
    await Promise.allSettled([...(applying ? [applying] : []), ...this.#activeCompiles, ...this.#drops.values()]);
    await Promise.allSettled([...this.#owned.values()].map((candidate) => this.#drop(candidate)));
    this.#current = this.#lastGood = undefined;
  }
}
