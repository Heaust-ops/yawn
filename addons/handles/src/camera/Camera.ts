import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

function rotate(q: ArrayLike<number>, value: number[]) {
  const [x, y, z] = value;
  const tx = 2 * (q[1] * z - q[2] * y);
  const ty = 2 * (q[2] * x - q[0] * z);
  const tz = 2 * (q[0] * y - q[1] * x);
  return [
    x + q[3] * tx + q[1] * tz - q[2] * ty,
    y + q[3] * ty + q[2] * tx - q[0] * tz,
    z + q[3] * tz + q[0] * ty - q[1] * tx,
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

  override get position(): Float32Array { return super.position; }
  override set position(value: ArrayLike<number>) {
    super.position = value;
    this.refreshMatrix();
  }

  override get quaternion(): Float32Array { return super.quaternion; }
  override set quaternion(value: ArrayLike<number>) {
    super.quaternion = value;
    this.refreshMatrix();
  }

  lookAt(target: ArrayLike<number>) {
    if (target.length !== 3) throw new RangeError("camera target");
    const position = this.position;
    const x = target[0] - position[0];
    const y = target[1] - position[1];
    const z = target[2] - position[2];
    const length = Math.hypot(x, y, z) || 1;
    const direction = [x / length, y / length, z / length];
    if (direction[2] > 0.999999) this.quaternion = [0, 1, 0, 0];
    else {
      const q = [direction[1], -direction[0], 0, 1 - direction[2]];
      const qLength = Math.hypot(...q);
      this.quaternion = q.map((lane) => lane / qLength);
    }
    return this;
  }

  protected refreshMatrix() {
    if (this.id < 0 || this.cameraId < 0) return;
    const camera = this.cameraRow();
    const position = this.position;
    const quaternion = this.quaternion;
    const right = rotate(quaternion, [1, 0, 0]);
    const up = rotate(quaternion, [0, 1, 0]);
    const forward = rotate(quaternion, [0, 0, 1]);
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
