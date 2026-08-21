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
  set alpha(value: number) {
    const row = this.cameraRow();
    this.#updateTransform(() => { row[13] = value; });
  }
  get beta() { return this.cameraRow()[14]; }
  set beta(value: number) {
    const row = this.cameraRow();
    this.#updateTransform(() => {
      row[14] = Math.min(Math.PI - 0.001, Math.max(0.001, value));
    });
  }
  get radius() { return this.cameraRow()[15]; }
  set radius(value: number) {
    const row = this.cameraRow();
    this.#updateTransform(() => { row[15] = Math.max(0.01, value); });
  }

  get target() { return this.#target; }
  set target(value: Node | undefined) {
    if (value && (value.scene !== this.scene || value.id < 0)) throw new Error("Target must be a ready Node in this Scene");
    this.#target = value;
    const row = this.cameraRow();
    this.#updateTransform(() => { row[12] = value ? value.id + 1 : 0; });
  }

  attachControls(controls: ArcRotateControls) {
    this.detachControls();
    this.#controls = controls;
    if (controls.pointer !== false) {
      controls.element.addEventListener("pointerdown", this.#down);
      controls.element.addEventListener("pointermove", this.#move);
      controls.element.addEventListener("pointerup", this.#up);
      controls.element.addEventListener("pointercancel", this.#up);
      controls.element.addEventListener("contextmenu", this.#contextMenu);
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
      element.removeEventListener("pointercancel", this.#up);
      element.removeEventListener("contextmenu", this.#contextMenu);
      element.removeEventListener("wheel", this.#wheel);
    }
    cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    this.#pointer = undefined;
    this.#controls = undefined;
  }

  #targetPosition() {
    const row = this.cameraRow();
    if (!this.#target) return row.subarray(16, 19);
    const position = this.#target.position;
    return [
      position.x + row[16],
      position.y + row[17],
      position.z + row[18],
    ];
  }

  #updateTransform(change?: () => void) {
    if (this.id < 0 || this.cameraId < 0) return;
    this.batchTransformChanges(() => {
      change?.();
      const target = this.#targetPosition();
      const sinBeta = Math.sin(this.beta);
      this.setPosition([
        target[0] + this.radius * sinBeta * Math.sin(this.alpha),
        target[1] + this.radius * Math.cos(this.beta),
        target[2] + this.radius * sinBeta * Math.cos(this.alpha),
      ]);
      this.lookAt(target);
    });
  }

  #down = (event: PointerEvent) => {
    event.preventDefault();
    this.#pointer = { x: event.clientX, y: event.clientY, button: event.button };
    this.#controls?.element.setPointerCapture?.(event.pointerId);
  };
  #up = () => { this.#pointer = undefined; };
  #contextMenu = (event: Event) => event.preventDefault();
  #move = (event: PointerEvent) => {
    if (!this.#pointer || !this.#controls) return;
    const x = event.clientX - this.#pointer.x;
    const y = event.clientY - this.#pointer.y;
    this.#pointer.x = event.clientX;
    this.#pointer.y = event.clientY;
    if (this.#pointer.button === 2) {
      const row = this.cameraRow();
      this.#updateTransform(() => {
        row[16] -= x * (this.#controls?.panSpeed ?? 0.005);
        row[17] += y * (this.#controls?.panSpeed ?? 0.005);
      });
    } else {
      const speed = this.#controls.orbitSpeed ?? 0.005;
      const row = this.cameraRow();
      this.#updateTransform(() => {
        row[13] -= x * speed;
        row[14] = Math.min(
          Math.PI - 0.001,
          Math.max(0.001, row[14] + y * speed),
        );
      });
    }
  };
  #wheel = (event: WheelEvent) => {
    event.preventDefault();
    const row = this.cameraRow();
    this.#updateTransform(() => {
      row[15] = Math.max(
        0.01,
        row[15] * Math.exp(event.deltaY * (this.#controls?.zoomSpeed ?? 0.001)),
      );
    });
  };
  #pollController = () => {
    const pad = navigator.getGamepads?.().find(Boolean);
    if (pad) {
      const row = this.cameraRow();
      this.#updateTransform(() => {
        row[13] += (pad.axes[2] ?? pad.axes[0] ?? 0) * 0.03;
        row[14] = Math.min(
          Math.PI - 0.001,
          Math.max(
            0.001,
            row[14] + (pad.axes[3] ?? pad.axes[1] ?? 0) * 0.03,
          ),
        );
        row[15] = Math.max(
          0.01,
          row[15] +
            ((pad.buttons[6]?.value ?? 0) -
              (pad.buttons[7]?.value ?? 0)) *
              0.1,
        );
      });
    }
    this.#frame = requestAnimationFrame(this.#pollController);
  };

  override async dispose() {
    this.detachControls();
    await super.dispose();
  }
}
