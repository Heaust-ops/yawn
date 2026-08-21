import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

function rotate(rotor: ArrayLike<number>, value: number[]) {
  const [x, y, z] = value;
  const tx = 2 * (rotor[1] * z - rotor[2] * y);
  const ty = 2 * (rotor[2] * x - rotor[0] * z);
  const tz = 2 * (rotor[0] * y - rotor[1] * x);
  return [
    x + rotor[3] * tx + rotor[1] * tz - rotor[2] * ty,
    y + rotor[3] * ty + rotor[2] * tx - rotor[0] * tz,
    z + rotor[3] * tz + rotor[0] * ty - rotor[1] * tx,
  ];
}

function dot(left: number[], right: ArrayLike<number>) {
  return left[0] * right[0] + left[1] * right[1] + left[2] * right[2];
}

export type CameraOptions = NodeOptions & {
  fov?: number;
  near?: number;
  far?: number;
  aspect?: number;
  projection?: "perspective" | "orthographic";
  orthoSize?: number;
  focalLength?: number;
  aperture?: number;
  focusDistance?: number;
  sensorWidth?: number;
};

/** Conventional camera data; core only sees another generically allocated shared row. */
export class Camera extends Node {
  cameraId = -1;
  #cameraDisposed = false;
  #transformBatchDepth = 0;
  #transformBatchDirty = false;

  constructor(scene: Scene, options: CameraOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      this.cameraId = await scene.core.allocateObject("cameras");
      await scene.ensureRows("cameraMatrices", this.cameraId + 1, 80, "f32");
      const row = scene.array("cameras").row(this.cameraId);
      row.set([
        options.fov ?? Math.PI / 3,
        options.aspect ?? 1,
        options.near ?? 0.1,
        options.far ?? 1000,
        this.id,
        options.projection === "orthographic" ? 1 : 0,
        options.orthoSize ?? 10,
        options.focalLength ?? 50,
        options.aperture ?? 2.8,
        options.focusDistance ?? 10,
        options.sensorWidth ?? 36,
        1,
      ]);
      this.refreshMatrix();
      return this;
    });
  }

  protected cameraRow() {
    if (this.cameraId < 0) throw new Error("Await camera.ready before reading or writing it");
    return this.scene.array("cameras").row(this.cameraId);
  }

  get fov() { return this.cameraRow()[0]; }
  set fov(value: number) { this.cameraRow()[0] = value; this.refreshMatrix(); }
  get aspect() { return this.cameraRow()[1]; }
  set aspect(value: number) { this.cameraRow()[1] = value; this.refreshMatrix(); }
  get near() { return this.cameraRow()[2]; }
  set near(value: number) { this.cameraRow()[2] = value; this.refreshMatrix(); }
  get far() { return this.cameraRow()[3]; }
  set far(value: number) { this.cameraRow()[3] = value; this.refreshMatrix(); }
  get projection() { return this.cameraRow()[5] === 1 ? "orthographic" : "perspective"; }
  set projection(value: "perspective" | "orthographic") {
    this.cameraRow()[5] = value === "orthographic" ? 1 : 0;
    this.refreshMatrix();
  }
  get orthoSize() { return this.cameraRow()[6]; }
  set orthoSize(value: number) { this.cameraRow()[6] = value; this.refreshMatrix(); }
  get focalLength() { return this.cameraRow()[7]; }
  set focalLength(value: number) { this.cameraRow()[7] = value; }
  get aperture() { return this.cameraRow()[8]; }
  set aperture(value: number) { this.cameraRow()[8] = value; }
  get focusDistance() { return this.cameraRow()[9]; }
  set focusDistance(value: number) { this.cameraRow()[9] = value; }
  get sensorWidth() { return this.cameraRow()[10]; }
  set sensorWidth(value: number) { this.cameraRow()[10] = value; }

  lookAt(target: ArrayLike<number>) {
    if (target.length !== 3) throw new RangeError("camera target");
    const position = this.position;
    const x = target[0] - position.x;
    const y = target[1] - position.y;
    const z = target[2] - position.z;
    const length = Math.hypot(x, y, z) || 1;
    const direction = [x / length, y / length, z / length];
    if (direction[2] > 0.999999) this.setRotor([0, 1, 0, 0]);
    else {
      const rotor = [direction[1], -direction[0], 0, 1 - direction[2]];
      const rotorLength = Math.hypot(...rotor);
      this.setRotor(rotor.map((lane) => lane / rotorLength));
    }
    return this;
  }

  protected override transformChanged() {
    if (this.#transformBatchDepth) {
      this.#transformBatchDirty = true;
      return;
    }
    this.refreshMatrix();
  }

  /** Publishes one final camera matrix after a synchronous group of transform writes. */
  protected batchTransformChanges<T>(operation: () => T) {
    return this.scene.batchWrites(() => {
      this.#transformBatchDepth++;
      try {
        return operation();
      } finally {
        this.#transformBatchDepth--;
        if (!this.#transformBatchDepth && this.#transformBatchDirty) {
          this.#transformBatchDirty = false;
          this.refreshMatrix();
        }
      }
    });
  }

  protected refreshMatrix() {
    if (this.id < 0 || this.cameraId < 0) return;
    const camera = this.cameraRow();
    const position = this.position;
    const rotor = this.rotor;
    const right = rotate(rotor, [1, 0, 0]);
    const up = rotate(rotor, [0, 1, 0]);
    const forward = rotate(rotor, [0, 0, 1]);
    let rows: number[][];
    if (camera[5] === 1) {
      const size = Math.max(camera[6], 0.0001);
      const x = 2 / (size * camera[1]);
      const y = 2 / size;
      const z = -1 / camera[3];
      rows = [
        [...right.map((value) => value * x), -dot(right, position) * x],
        [...up.map((value) => value * y), -dot(up, position) * y],
        [...forward.map((value) => value * z), -dot(forward, position) * z],
        [0, 0, 0, 1],
      ];
    } else {
      const focal = 1 / Math.tan(camera[0] * 0.5);
      const x = focal / camera[1];
      const z = -camera[3] / (camera[3] - camera[2]);
      const translation = (-camera[2] * camera[3]) / (camera[3] - camera[2]);
      rows = [
        [...right.map((value) => value * x), -dot(right, position) * x],
        [...up.map((value) => value * focal), -dot(up, position) * focal],
        [
          ...forward.map((value) => value * z),
          -dot(forward, position) * z + translation,
        ],
        [...forward.map((value) => -value), dot(forward, position)],
      ];
    }
    this.scene
      .array("cameraMatrices")
      .row(this.cameraId)
      .set([...rows.flat(), ...position, camera[11]]);
  }

  override async dispose() {
    await this.ready;
    if (this.#cameraDisposed) return;
    this.#cameraDisposed = true;
    await this.scene.core.deleteObject("cameras", this.cameraId);
    await super.dispose();
  }
}
