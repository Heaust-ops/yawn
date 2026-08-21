import type { Node } from "../Node";
import type { Scene } from "../Scene";
import { Camera, type CameraOptions } from "./Camera";

export type FollowCameraOptions = CameraOptions & {
  target: Node;
  distance?: number;
  height?: number;
  smoothing?: number;
  running?: boolean;
};

/** Third-person camera that follows a target Node by direct shared-row reads and writes. */
export class FollowCamera extends Camera {
  #target: Node;
  #frame = 0;
  #smoothing: number;

  constructor(scene: Scene, options: FollowCameraOptions) {
    if (!options?.target) throw new TypeError("FollowCamera target is required");
    super(scene, options);
    this.#target = options.target;
    this.#smoothing = options.smoothing ?? 0.12;
    const cameraReady = this.ready;
    this.ready = cameraReady.then(async () => {
      await this.#target.ready;
      if (this.#target.scene !== scene) throw new Error("Target must belong to this Scene");
      const row = this.cameraRow();
      row[12] = this.#target.id + 1;
      row[15] = options.distance ?? 6;
      row[16] = options.height ?? 2;
      row[19] = 3;
      this.#snap();
      if (options.running !== false) this.start();
      return this;
    });
  }

  get target() { return this.#target; }
  set target(value: Node) {
    if (value.scene !== this.scene || value.id < 0) throw new Error("Target must be a ready Node in this Scene");
    this.#target = value;
    this.cameraRow()[12] = value.id + 1;
  }
  get distance() { return this.cameraRow()[15]; }
  set distance(value: number) { this.cameraRow()[15] = Math.max(0, value); }
  get height() { return this.cameraRow()[16]; }
  set height(value: number) { this.cameraRow()[16] = value; }
  get smoothing() { return this.#smoothing; }
  set smoothing(value: number) { this.#smoothing = Math.min(1, Math.max(0, value)); }

  start() {
    if (!this.#frame) this.#frame = requestAnimationFrame(this.#follow);
    return this;
  }

  stop() {
    cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    return this;
  }

  #snap() {
    const target = this.#target.position;
    this.setPosition([target.x, target.y + this.height, target.z + this.distance]);
    this.lookAt(target);
  }

  #follow = () => {
    const target = this.#target.position;
    const position = this.position;
    this.setPosition([
      position.x + (target.x - position.x) * this.#smoothing,
      position.y + (target.y + this.height - position.y) * this.#smoothing,
      position.z + (target.z + this.distance - position.z) * this.#smoothing,
    ]);
    this.lookAt(target);
    this.#frame = requestAnimationFrame(this.#follow);
  };

  override async dispose() {
    this.stop();
    await super.dispose();
  }
}
