export class GltfImportError extends Error {
  constructor(code) {
    super(code);
    this.name = "GltfImportError";
    this.code = code;
  }
}

function frameCamera(core, bounds, framing) {
  if (!bounds || framing === false) return;
  if (framing !== undefined && framing !== "exterior" && framing !== "interior")
    throw new TypeError("framing must be exterior, interior, or false");
  const min = bounds.min, max = bounds.max;
  if (!Array.isArray(min) || !Array.isArray(max) || min.length !== 3 || max.length !== 3) return;
  const center = min.map((value, axis) => (value + max[axis]) * 0.5);
  const extent = max.map((value, axis) => value - min[axis]);
  const radius = Math.max(1, Math.hypot(...extent) * 0.5);
  const camera = core.array("camera.state");
  const state = camera.read(0);
  const interior = framing === "interior";
  const eye = interior
    ? [center[0], center[1] + radius * 0.05, center[2]]
    : [center[0] + radius * 1.8, center[1] + radius * 1.4, center[2] + radius * 1.8];
  const target = interior ? [center[0] + radius, center[1], center[2]] : center;
  state.splice(0, 3, ...eye);
  state.splice(4, 3, ...target);
  state.splice(8, 3, 0, 1, 0);
  state[14] = Math.max(radius * 0.001, 0.1);
  state[15] = Math.max(radius * 6, 1.1);
  camera.write(0, state);
}

/** Parses glTF in a dedicated worker and publishes a generic render-data packet through shared memory. */
export class GltfImporter {
  #core;
  #worker;
  #next = 1;
  #pending = new Map();
  #tail = Promise.resolve();
  #disposed = false;

  constructor(core, { workerFactory } = {}) {
    if (!core?.allocateArray || !core?.commitRenderDataUpload)
      throw new TypeError("core must implement the Yawn shared render-data protocol");
    this.#core = core;
    this.#worker = workerFactory
      ? workerFactory()
      : new Worker(new URL("./worker.js", import.meta.url), {
          type: "module",
          name: "yawn-gltf-import",
        });
    this.#worker.addEventListener("message", event => this.#message(event.data));
    this.#worker.addEventListener("error", () => this.#fail("GLTF_WORKER_ERROR"));
    this.#worker.addEventListener("messageerror", () => this.#fail("GLTF_WORKER_ERROR"));
    this.#worker.start?.();
  }

  load(url, options = {}) {
    if (this.#disposed) return Promise.reject(new GltfImportError("DISPOSED"));
    const source = url instanceof URL ? url.href : url;
    if (typeof source !== "string" || !source) return Promise.reject(new TypeError("url must be a URL or nonempty string"));
    const operation = this.#tail.then(() => this.#start(source, options));
    this.#tail = operation.catch(() => {});
    return operation;
  }

  #start(url, options) {
    let request = this.#next++ >>> 0;
    if (!request) request = this.#next++ >>> 0;
    return new Promise((resolve, reject) => {
      this.#pending.set(request, { resolve, reject, options, array: null });
      this.#worker.postMessage({ type: "load", request, url });
    });
  }

  async #message(message) {
    const pending = this.#pending.get(message?.request);
    if (!pending) return;
    try {
      if (message.type === "allocate") {
        const length = Math.ceil(message.byteLength / 16);
        pending.array = await this.#core.allocateArray({
          name: "upload.renderData",
          domain: "fixed",
          scalar: "u32",
          lanes: 4,
          stride: 16,
          length,
        });
        this.#worker.postMessage({
          type: "storage",
          request: message.request,
          ...pending.array.share(),
        });
      } else if (message.type === "ready") {
        const result = await this.#core.commitRenderDataUpload(
          pending.array,
          message.byteLength,
        );
        frameCamera(this.#core, result.bounds, pending.options.framing);
        this.#pending.delete(message.request);
        pending.resolve(result);
      } else if (message.type === "error") {
        this.#pending.delete(message.request);
        pending.reject(new GltfImportError(message.code || "GLTF_IMPORT_FAILED"));
      }
    } catch (error) {
      this.#pending.delete(message.request);
      pending.reject(error);
    }
  }

  #fail(code) {
    if (this.#disposed) return;
    this.#disposed = true;
    const error = new GltfImportError(code);
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
    this.#worker.terminate?.();
  }

  dispose() {
    if (this.#disposed) return;
    this.#fail("DISPOSED");
  }
}
