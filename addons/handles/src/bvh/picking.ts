import type { Scene } from "../Scene";

export type PickHit = { id: number; distance: number };

/** Worker-backed broad-phase picking that returns every SAB AABB hit, nearest first. */
export class Picking {
  readonly scene: Scene;
  readonly ready: Promise<void>;
  #worker: Worker;
  #next = 1;
  #pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();

  constructor(scene: Scene) {
    this.scene = scene;
    this.#worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module", name: "yawn-bvh" });
    this.#worker.addEventListener("message", ({ data }) => {
      const pending = this.#pending.get(data.request);
      if (!pending) return;
      this.#pending.delete(data.request);
      pending.resolve(data.hits);
    });
    this.#worker.addEventListener("error", () => this.#fail(new Error("BVH_WORKER_ERROR")));
    this.ready = scene.ready.then(() => this.refresh());
  }

  refresh() {
    return this.#request("sync", {
      shares: Object.fromEntries(["info", "nodes", "nodePositions", "meshInfo", "bounds"]
        .map((name) => [name, this.scene.array(name).share()])),
    }).then(() => undefined);
  }

  async pick(origin: ArrayLike<number>, direction: ArrayLike<number>): Promise<PickHit[]> {
    await this.ready;
    if (origin.length !== 3 || direction.length !== 3) throw new TypeError("Pick rays have three lanes");
    return this.#request("pick", { origin: Array.from(origin), direction: Array.from(direction) });
  }

  #request(type: string, payload: object) {
    const request = this.#next++;
    return new Promise<any>((resolve, reject) => {
      this.#pending.set(request, { resolve, reject });
      this.#worker.postMessage({ type, request, ...payload });
    });
  }

  #fail(error: Error) {
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
  }

  dispose() {
    this.#fail(new Error("DISPOSED"));
    this.#worker.terminate();
  }
}
