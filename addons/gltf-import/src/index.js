/** Fetches and parses glTF in a worker, then lets that worker write the packet into Yawn's SAB arena. */
export class GltfImporter {
  #core;
  #worker;
  #next = 1;
  #pending = new Map();

  constructor(core, { workerFactory } = {}) {
    if (!core?.allocateRows) throw new TypeError("core must be a YawnCore instance");
    this.#core = core;
    this.#worker = workerFactory?.() ?? new Worker(new URL("./worker.js", import.meta.url), {
      type: "module",
      name: "yawn-gltf-import",
    });
    this.#worker.addEventListener("message", ({ data }) => this.#message(data));
    this.#worker.addEventListener("error", () => this.#fail("GLTF_WORKER_ERROR"));
    this.#worker.start?.();
  }

  load(url) {
    const source = url instanceof URL ? url.href : url;
    if (typeof source !== "string" || !source) throw new TypeError("url is required");
    const request = this.#next++;
    return new Promise((resolve, reject) => {
      this.#pending.set(request, { resolve, reject });
      this.#worker.postMessage({ type: "load", request, url: source });
    });
  }

  async #message(message) {
    const pending = this.#pending.get(message?.request);
    if (!pending) return;
    try {
      if (message.type === "allocate") {
        pending.array = await this.#core.allocateRows({
          name: `gltf.${message.request}`,
          rows: Math.ceil(message.byteLength / 16),
          stride: 16,
          format: "u32",
        });
        this.#worker.postMessage({ type: "storage", request: message.request, ...pending.array.share() });
      } else {
        this.#pending.delete(message.request);
        if (message.type === "ready") pending.resolve({ array: pending.array, byteLength: message.byteLength });
        else pending.reject(new Error(message.error ?? "GLTF_IMPORT_FAILED"));
      }
    } catch (error) {
      this.#pending.delete(message.request);
      pending.reject(error);
    }
  }

  #fail(code) {
    for (const { reject } of this.#pending.values()) reject(new Error(code));
    this.#pending.clear();
  }

  dispose() {
    this.#fail("DISPOSED");
    this.#worker.terminate();
  }
}
