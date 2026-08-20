import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

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
      return this;
    });
  }

  protected cameraRow() {
    if (this.cameraId < 0) throw new Error("Await camera.ready before reading or writing it");
    return this.scene.array("cameras").row(this.cameraId);
  }

  get fov() { return this.cameraRow()[0]; }
  set fov(value: number) { this.cameraRow()[0] = value; }
  get aspect() { return this.cameraRow()[1]; }
  set aspect(value: number) { this.cameraRow()[1] = value; }
  get near() { return this.cameraRow()[2]; }
  set near(value: number) { this.cameraRow()[2] = value; }
  get far() { return this.cameraRow()[3]; }
  set far(value: number) { this.cameraRow()[3] = value; }
  get projection() { return this.cameraRow()[5] === 1 ? "orthographic" : "perspective"; }
  set projection(value: "perspective" | "orthographic") { this.cameraRow()[5] = value === "orthographic" ? 1 : 0; }
  get orthoSize() { return this.cameraRow()[6]; }
  set orthoSize(value: number) { this.cameraRow()[6] = value; }
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

  override async dispose() {
    await this.ready;
    if (this.#cameraDisposed) return;
    this.#cameraDisposed = true;
    await this.scene.core.deleteObject("cameras", this.cameraId);
    await super.dispose();
  }
}
