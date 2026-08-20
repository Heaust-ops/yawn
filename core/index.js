const views = Object.freeze({ f32: Float32Array, u32: Uint32Array, i32: Int32Array });

export class SharedRows {
  constructor(buffer, descriptor) {
    if (!(buffer instanceof SharedArrayBuffer)) throw new TypeError("ROWS_DESCRIPTOR");
    this.buffer = buffer;
    this.update(descriptor);
  }

  update(descriptor) {
    if (!views[descriptor.format] || descriptor.name !== (this.descriptor?.name ?? descriptor.name))
      throw new TypeError("ROWS_DESCRIPTOR");
    this.descriptor = Object.freeze(descriptor);
    return this;
  }

  get name() { return this.descriptor.name; }
  get rows() { return this.descriptor.rows; }
  get stride() { return this.descriptor.stride; }
  get format() { return this.descriptor.format; }
  get view() {
    const View = views[this.format];
    return new View(this.buffer, this.descriptor.offset, this.descriptor.bytes / View.BYTES_PER_ELEMENT);
  }

  row(index) {
    if (!Number.isInteger(index) || index < 0 || index >= this.rows) throw new RangeError("ROW_RANGE");
    const width = this.stride / views[this.format].BYTES_PER_ELEMENT;
    return this.view.subarray(index * width, (index + 1) * width);
  }

  read(index) { return Array.from(this.row(index)); }

  write(index, values) {
    const row = this.row(index);
    if (!values || values.length !== row.length) throw new RangeError("ROW_WIDTH");
    row.set(values);
    return this;
  }

  share() { return { buffer: this.buffer, descriptor: this.descriptor }; }
}

export class YawnCore {
  #worker;
  #buffer;
  #arrays = new Map();
  #pending = new Map();
  #next = 1;

  constructor(canvas, { arenaBytes = 64 * 1024 * 1024, workerFactory } = {}) {
    if (!canvas) throw new TypeError("canvas is required");
    this.#worker = workerFactory?.() ?? new Worker(new URL("./worker.js", import.meta.url), {
      type: "module",
      name: "yawn-core",
    });
    this.#worker.addEventListener("message", ({ data }) => this.#message(data));
    this.#worker.addEventListener("error", () => this.#fail("WORKER_ERROR"));
    this.#worker.addEventListener("messageerror", () => this.#fail("WORKER_ERROR"));
    this.#worker.start?.();
    const offscreen = canvas.transferControlToOffscreen?.() ?? canvas;
    this.ready = this.#request("init", { canvas: offscreen, arenaBytes }, [offscreen]).then(result => {
      this.#buffer = result.buffer;
      for (const descriptor of result.rows) this.#arrays.set(
        descriptor.name,
        new SharedRows(this.#buffer, descriptor),
      );
    });
  }

  async createRows({ name, rows, stride, format }) {
    await this.ready;
    const descriptor = await this.#request("create-rows", { name, rows, stride, format });
    const array = this.#arrays.get(name)?.update(descriptor)
      ?? new SharedRows(this.#buffer, descriptor);
    this.#arrays.set(name, array);
    return array;
  }

  async deleteRows(name) {
    await this.ready;
    await this.#request("delete-rows", { name });
    this.#arrays.delete(name);
  }

  async allocateObject(name) {
    await this.ready;
    const { id, rows } = await this.#request("allocate-object", { name });
    this.#arrays.get(name).update(rows);
    return id;
  }

  async deleteObject(name, id) {
    await this.ready;
    return this.#request("delete-object", { name, id });
  }

  async compileGraph(serialized) {
    await this.ready;
    return this.#request("compile-graph", { serialized });
  }

  async switchLoadout(id) {
    await this.ready;
    return this.#request("switch-loadout", { id });
  }

  async play() {
    await this.ready;
    return this.#request("play");
  }

  async pause() {
    await this.ready;
    return this.#request("pause");
  }

  async setFps(fps) {
    await this.ready;
    return this.#request("set-fps", { fps });
  }

  array(name) {
    const array = this.#arrays.get(name);
    if (!array) throw new Error(`UNKNOWN_ARRAY: ${name}`);
    return array;
  }

  #request(type, payload = {}, transfer = []) {
    const request = this.#next++;
    return new Promise((resolve, reject) => {
      this.#pending.set(request, { resolve, reject });
      this.#worker.postMessage({ type, request, ...payload }, transfer);
    });
  }

  #message(message) {
    const pending = this.#pending.get(message?.request);
    if (!pending) return;
    this.#pending.delete(message.request);
    if (message.error) pending.reject(Object.assign(new Error(message.error), { code: message.error }));
    else pending.resolve(message.result);
  }

  #fail(code) {
    for (const { reject } of this.#pending.values()) reject(Object.assign(new Error(code), { code }));
    this.#pending.clear();
  }

  dispose() {
    this.#fail("DISPOSED");
    this.#worker.terminate();
  }
}
