import type { Scene } from "./Scene";

export type NodeOptions = {
  position?: ArrayLike<number>;
  quaternion?: ArrayLike<number>;
  scale?: ArrayLike<number>;
  parent?: Node;
};

function vector(value: ArrayLike<number>, width: number, name: string) {
  if (value.length !== width || Array.from(value).some((lane) => !Number.isFinite(lane)))
    throw new TypeError(name);
  return value;
}

/** A thin index into transform SOA rows; transform changes never post a worker message. */
export class Node {
  readonly scene: Scene;
  id = -1;
  ready: Promise<this>;
  protected disposed = false;

  constructor(scene: Scene, options: NodeOptions = {}) {
    this.scene = scene;
    this.ready = scene.allocateNode().then((id) => {
      this.id = id;
      if (options.position) this.position = options.position;
      if (options.quaternion) this.quaternion = options.quaternion;
      if (options.scale) this.scale = options.scale;
      if (options.parent) this.parent = options.parent;
      return this;
    });
  }

  #row(name: string) {
    if (this.id < 0) throw new Error("Await node.ready before reading or writing it");
    return this.scene.array(name).row(this.id);
  }

  get position(): Float32Array { return this.#row("nodePositions").subarray(0, 3); }
  set position(value: ArrayLike<number>) { this.#row("nodePositions").set(vector(value, 3, "position")); }

  get quaternion(): Float32Array { return this.#row("nodeQuaternions").subarray(0, 4); }
  set quaternion(value: ArrayLike<number>) { this.#row("nodeQuaternions").set(vector(value, 4, "quaternion")); }

  get scale(): Float32Array { return this.#row("nodeScales").subarray(0, 3); }
  set scale(value: ArrayLike<number>) { this.#row("nodeScales").set(vector(value, 3, "scale")); }

  get enabled() { return this.#row("nodes")[0] !== 0; }
  set enabled(value: boolean) { this.#row("nodes")[0] = value ? 1 : 0; }

  get parentId() {
    const pointer = this.#row("nodes")[1];
    return pointer ? pointer - 1 : null;
  }

  set parent(value: Node | null) {
    if (value && value.scene !== this.scene) throw new Error("Nodes must belong to the same Scene");
    if (value && value.id < 0) throw new Error("Await parent.ready before assigning it");
    this.#row("nodes")[1] = value ? value.id + 1 : 0;
  }

  async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.disposed = true;
    await this.scene.releaseNode(this.id);
  }
}
