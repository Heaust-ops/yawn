import type { Node } from "../Node";
import type { Scene } from "../Scene";
import { Camera, type CameraOptions } from "./Camera";

export type ArcRotateControls = {
  element: HTMLElement;
  pointer?: boolean;
  controller?: boolean;
  orbitSpeed?: number;
  panSpeed?: number;
  zoomSpeed?: number;
};
export type ArcRotateCameraOptions = CameraOptions & {
  target?: Node;
  targetPosition?: ArrayLike<number>;
  alpha?: number;
  beta?: number;
  radius?: number;
  controls?: ArcRotateControls;
};

/** Orbit/pan/zoom camera whose controls mutate only its transform and camera SAB rows. */
export class ArcRotateCamera extends Camera {
  #target?: Node;
  #controls?: ArcRotateControls;
  #pointer?: { x: number; y: number; button: number };
  #frame = 0;

  constructor(scene: Scene, options: ArcRotateCameraOptions = {}) {
    super(scene, options);
    this.#target = options.target;
    const cameraReady = this.ready;
    this.ready = cameraReady.then(async () => {
      if (this.#target) await this.#target.ready;
      const row = this.cameraRow();
      row[12] = this.#target ? this.#target.id + 1 : 0;
      row[13] = options.alpha ?? 0;
      row[14] = options.beta ?? Math.PI / 3;
      row[15] = options.radius ?? 5;
      row.set(Array.from(options.targetPosition ?? [0, 0, 0]), 16);
      row[19] = 1;
      this.#updateTransform();
      if (options.controls) this.attachControls(options.controls);
      return this;
    });
  }

  get alpha() { return this.cameraRow()[13]; }
  set alpha(value: number) { this.cameraRow()[13] = value; this.#updateTransform(); }
  get beta() { return this.cameraRow()[14]; }
  set beta(value: number) { this.cameraRow()[14] = Math.min(Math.PI - 0.001, Math.max(0.001, value)); this.#updateTransform(); }
  get radius() { return this.cameraRow()[15]; }
  set radius(value: number) { this.cameraRow()[15] = Math.max(0.01, value); this.#updateTransform(); }

  get target() { return this.#target; }
  set target(value: Node | undefined) {
    if (value && (value.scene !== this.scene || value.id < 0)) throw new Error("Target must be a ready Node in this Scene");
    this.#target = value;
    this.cameraRow()[12] = value ? value.id + 1 : 0;
    this.#updateTransform();
  }

  attachControls(controls: ArcRotateControls) {
    this.detachControls();
    this.#controls = controls;
    if (controls.pointer !== false) {
      controls.element.addEventListener("pointerdown", this.#down);
      controls.element.addEventListener("pointermove", this.#move);
      controls.element.addEventListener("pointerup", this.#up);
      controls.element.addEventListener("wheel", this.#wheel, { passive: false });
    }
    if (controls.controller) this.#frame = requestAnimationFrame(this.#pollController);
    return this;
  }

  detachControls() {
    const element = this.#controls?.element;
    if (element) {
      element.removeEventListener("pointerdown", this.#down);
      element.removeEventListener("pointermove", this.#move);
      element.removeEventListener("pointerup", this.#up);
      element.removeEventListener("wheel", this.#wheel);
    }
    cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    this.#pointer = undefined;
    this.#controls = undefined;
  }

  #targetPosition() {
    return this.#target ? this.#target.position : this.cameraRow().subarray(16, 19);
  }

  #updateTransform() {
    if (this.id < 0 || this.cameraId < 0) return;
    const target = this.#targetPosition();
    const sinBeta = Math.sin(this.beta);
    this.position = [
      target[0] + this.radius * sinBeta * Math.sin(this.alpha),
      target[1] + this.radius * Math.cos(this.beta),
      target[2] + this.radius * sinBeta * Math.cos(this.alpha),
    ];
    this.lookAt(target);
  }

  #down = (event: PointerEvent) => {
    this.#pointer = { x: event.clientX, y: event.clientY, button: event.button };
    this.#controls?.element.setPointerCapture?.(event.pointerId);
  };
  #up = () => { this.#pointer = undefined; };
  #move = (event: PointerEvent) => {
    if (!this.#pointer || !this.#controls) return;
    const x = event.clientX - this.#pointer.x;
    const y = event.clientY - this.#pointer.y;
    this.#pointer.x = event.clientX;
    this.#pointer.y = event.clientY;
    if (this.#pointer.button === 2) {
      const target = this.#targetPosition();
      target[0] -= x * (this.#controls.panSpeed ?? 0.005);
      target[1] += y * (this.#controls.panSpeed ?? 0.005);
      this.#updateTransform();
    } else {
      const speed = this.#controls.orbitSpeed ?? 0.005;
      this.cameraRow()[13] -= x * speed;
      this.cameraRow()[14] = Math.min(Math.PI - 0.001, Math.max(0.001, this.beta + y * speed));
      this.#updateTransform();
    }
  };
  #wheel = (event: WheelEvent) => {
    event.preventDefault();
    this.radius *= Math.exp(event.deltaY * (this.#controls?.zoomSpeed ?? 0.001));
  };
  #pollController = () => {
    const pad = navigator.getGamepads?.().find(Boolean);
    if (pad) {
      this.cameraRow()[13] += (pad.axes[2] ?? pad.axes[0] ?? 0) * 0.03;
      this.cameraRow()[14] = Math.min(Math.PI - 0.001, Math.max(0.001, this.beta + (pad.axes[3] ?? pad.axes[1] ?? 0) * 0.03));
      this.radius += ((pad.buttons[6]?.value ?? 0) - (pad.buttons[7]?.value ?? 0)) * 0.1;
      this.#updateTransform();
    }
    this.#frame = requestAnimationFrame(this.#pollController);
  };

  override async dispose() {
    this.detachControls();
    await super.dispose();
  }
}
