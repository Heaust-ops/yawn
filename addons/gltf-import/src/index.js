export class GltfImportError extends Error {
  constructor(code) {
    super(code);
    this.name = "GltfImportError";
    this.code = code;
  }
}

/** Fetches glTF in a dedicated worker and commits only shared-memory upload metadata. */
export class GltfImporter {
  #core;
  #worker;
  #next = 1;
  #pending = new Map();
  #tail = Promise.resolve();
  #disposed = false;

  constructor(core, { workerFactory } = {}) {
    if (!core?.allocateArray || !core?.commitGlbUpload) throw new TypeError("core must implement the Yawn shared upload protocol");
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
          name: "upload.gltf",
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
        const result = await this.#core.commitGlbUpload(
          pending.array,
          message.byteLength,
          pending.options,
        );
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
