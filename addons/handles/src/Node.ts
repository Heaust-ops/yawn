import type { Scene } from "./Scene";

export type NodeOptions = {
  position?: ArrayLike<number>;
  rotor?: ArrayLike<number>;
  scale?: ArrayLike<number>;
  parent?: Node;
};

function vector(value: ArrayLike<number>, width: number, name: string) {
  const lanes = Array.from(value);
  if (lanes.length !== width || lanes.some((lane) => !Number.isFinite(lane)))
    throw new TypeError(name);
  return lanes;
}

/** Three shared transform components. Component writes go straight to the backing SAB row. */
export interface Vector3 extends Iterable<number> {
  [lane: number]: number;
  readonly length: 3;
  x: number;
  y: number;
  z: number;
  toArray(): [number, number, number];
}

/** Four shared rotor components in `[x, y, z, w]` order. */
export interface Rotor extends Iterable<number> {
  [lane: number]: number;
  readonly length: 4;
  x: number;
  y: number;
  z: number;
  w: number;
  toArray(): [number, number, number, number];
}

abstract class XYZView {
  [lane: number]: number;
  abstract readonly length: 3 | 4;

  constructor(
    private readonly row: () => Float32Array,
    private readonly changed: () => void,
  ) {}

  protected read(lane: number) { return this.row()[lane]; }
  protected write(lane: number, value: number) {
    this.row()[lane] = value;
    this.changed();
  }

  get 0() { return this.read(0); }
  set 0(value: number) { this.write(0, value); }
  get 1() { return this.read(1); }
  set 1(value: number) { this.write(1, value); }
  get 2() { return this.read(2); }
  set 2(value: number) { this.write(2, value); }
  get x() { return this.read(0); }
  set x(value: number) { this.write(0, value); }
  get y() { return this.read(1); }
  set y(value: number) { this.write(1, value); }
  get z() { return this.read(2); }
  set z(value: number) { this.write(2, value); }

  abstract toArray(): number[];
  [Symbol.iterator]() { return this.toArray().values(); }
}

class Vector3View extends XYZView implements Vector3 {
  readonly length = 3;
  toArray(): [number, number, number] { return [this.x, this.y, this.z]; }
}

class RotorView extends XYZView implements Rotor {
  readonly length = 4;
  get 3() { return this.read(3); }
  set 3(value: number) { this.write(3, value); }
  get w() { return this.read(3); }
  set w(value: number) { this.write(3, value); }
  toArray(): [number, number, number, number] {
    return [this.x, this.y, this.z, this.w];
  }
}

/** A thin index into transform SOA rows; transform changes never post a worker message. */
export class Node {
  readonly scene: Scene;
  id = -1;
  ready: Promise<this>;
  protected disposed = false;
  readonly #position = new Vector3View(
    () => this.#row("nodePositions").subarray(0, 3),
    () => this.transformChanged(),
  );
  readonly #rotor = new RotorView(
    () => this.#row("nodeRotors").subarray(0, 4),
    () => this.transformChanged(),
  );
  readonly #scale = new Vector3View(
    () => this.#row("nodeScales").subarray(0, 3),
    () => this.transformChanged(),
  );

  constructor(scene: Scene, options: NodeOptions = {}) {
    this.scene = scene;
    this.ready = scene.allocateNode().then((id) => {
      this.id = id;
      if (options.position) this.setPosition(options.position);
      if (options.rotor) this.setRotor(options.rotor);
      if (options.scale) this.setScale(options.scale);
      if (options.parent) this.parent = options.parent;
      return this;
    });
  }

  #row(name: string) {
    if (this.id < 0) throw new Error("Await node.ready before reading or writing it");
    return this.scene.array(name).row(this.id);
  }

  get position(): Vector3 { return this.#position; }
  set position(value: ArrayLike<number>) { this.setPosition(value); }

  get rotor(): Rotor { return this.#rotor; }
  set rotor(value: ArrayLike<number>) { this.setRotor(value); }

  get scale(): Vector3 { return this.#scale; }
  set scale(value: ArrayLike<number>) { this.setScale(value); }

  /** Replaces all position components with one SAB row write. */
  setPosition(value: ArrayLike<number>) {
    this.#row("nodePositions").set(vector(value, 3, "position"));
    this.transformChanged();
    return this;
  }

  /** Replaces all rotor components with one SAB row write. */
  setRotor(value: ArrayLike<number>) {
    this.#row("nodeRotors").set(vector(value, 4, "rotor"));
    this.transformChanged();
    return this;
  }

  /** Replaces all scale components with one SAB row write. */
  setScale(value: ArrayLike<number>) {
    this.#row("nodeScales").set(vector(value, 3, "scale"));
    this.transformChanged();
    return this;
  }

  /** Adds an offset to the shared position. */
  translate(offset: ArrayLike<number>) {
    const [x, y, z] = vector(offset, 3, "translation");
    return this.setPosition([
      this.position.x + x,
      this.position.y + y,
      this.position.z + z,
    ]);
  }

  /** Composes a rotor, or an axis and angle in radians, onto the current rotor. */
  rotate(rotor: ArrayLike<number>): this;
  rotate(axis: ArrayLike<number>, radians: number): this;
  rotate(value: ArrayLike<number>, radians?: number) {
    let rotation: number[];
    if (radians === undefined) {
      rotation = vector(value, 4, "rotor");
    } else {
      if (!Number.isFinite(radians)) throw new TypeError("radians");
      const [x, y, z] = vector(value, 3, "axis");
      const length = Math.hypot(x, y, z);
      if (!length) throw new RangeError("axis");
      const sine = Math.sin(radians / 2) / length;
      rotation = [x * sine, y * sine, z * sine, Math.cos(radians / 2)];
    }
    const [ax, ay, az, aw] = this.rotor;
    const [bx, by, bz, bw] = rotation;
    const result = [
      aw * bx + ax * bw + ay * bz - az * by,
      aw * by - ax * bz + ay * bw + az * bx,
      aw * bz + ax * by - ay * bx + az * bw,
      aw * bw - ax * bx - ay * by - az * bz,
    ];
    const length = Math.hypot(...result);
    if (!length) throw new RangeError("rotor");
    return this.setRotor(result.map((lane) => lane / length));
  }

  /** Rotates around the node's local X axis. */
  rotateX(radians: number) { return this.rotate([1, 0, 0], radians); }
  /** Rotates around the node's local Y axis. */
  rotateY(radians: number) { return this.rotate([0, 1, 0], radians); }
  /** Rotates around the node's local Z axis. */
  rotateZ(radians: number) { return this.rotate([0, 0, 1], radians); }

  protected transformChanged() {}

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
