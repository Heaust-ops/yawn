const TOKEN = Symbol("yawn mesh handle addon");

/** Creates the optional snapshot/BVH worker used by core's picking protocol. */
export const createPickingWorker = () => new Worker(
  new URL("./bvh-worker.js", import.meta.url),
  { type: "module", name: "yawn-spatial-query" },
);

/** Conventional mesh/instance objects layered entirely over the Yawn core protocol. */
export class MeshHandles {
  #core;

  constructor(core) {
    this.#core = core;
  }

  fromImportedScene(result) {
    if (!result || !Array.isArray(result.meshes)) throw new TypeError("invalid imported scene");
    return result.meshes.map((mesh) => new Mesh(TOKEN, this.#core, mesh));
  }

  async pickRay(origin, direction, options) {
    const result = await this.#core.pickRay(origin, direction, options);
    return {
      ...result,
      hits: result.hits.map((hit) => ({
        ...hit,
        instance: new Instance(TOKEN, this.#core, hit.instance),
      })),
    };
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
