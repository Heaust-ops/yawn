import type { Scene } from "../Scene";
import { Camera, type CameraOptions } from "./Camera";

export type FreeCameraControls = {
  element: HTMLElement;
  keyboard?: boolean;
  pointer?: boolean;
  controller?: boolean;
  speed?: number;
  sensitivity?: number;
};
export type FreeCameraOptions = CameraOptions & { controls?: FreeCameraControls };

/** Spectator-style WASD/mouse/gamepad camera backed entirely by shared camera and transform rows. */
export class FreeCamera extends Camera {
  #controls?: FreeCameraControls;
  #keys = new Set<string>();
  #frame = 0;
  #last = 0;

  constructor(scene: Scene, options: FreeCameraOptions = {}) {
    super(scene, options);
    const cameraReady = this.ready;
    this.ready = cameraReady.then(() => {
      this.cameraRow()[19] = 2;
      if (options.controls) this.attachControls(options.controls);
      return this;
    });
  }

  attachControls(controls: FreeCameraControls) {
    this.detachControls();
    this.#controls = controls;
    if (controls.keyboard !== false) {
      addEventListener("keydown", this.#keyDown);
      addEventListener("keyup", this.#keyUp);
    }
    if (controls.pointer !== false) {
      controls.element.addEventListener("click", this.#lock);
      addEventListener("mousemove", this.#mouse);
    }
    this.#last = performance.now();
    this.#frame = requestAnimationFrame(this.#update);
    return this;
  }

  detachControls() {
    const element = this.#controls?.element;
    if (element) element.removeEventListener("click", this.#lock);
    removeEventListener("keydown", this.#keyDown);
    removeEventListener("keyup", this.#keyUp);
    removeEventListener("mousemove", this.#mouse);
    cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    this.#keys.clear();
    this.#controls = undefined;
  }

  #keyDown = (event: KeyboardEvent) => this.#keys.add(event.code);
  #keyUp = (event: KeyboardEvent) => this.#keys.delete(event.code);
  #lock = () => this.#controls?.element.requestPointerLock?.();
  #mouse = (event: MouseEvent) => {
    if (!this.#controls || document.pointerLockElement !== this.#controls.element) return;
    const sensitivity = this.#controls.sensitivity ?? 0.002;
    this.cameraRow()[13] -= event.movementX * sensitivity;
    this.cameraRow()[14] = Math.min(1.55, Math.max(-1.55, this.cameraRow()[14] - event.movementY * sensitivity));
    this.#writeQuaternion();
  };

  #writeQuaternion() {
    const yaw = this.cameraRow()[13];
    const pitch = this.cameraRow()[14];
    const sy = Math.sin(yaw / 2), cy = Math.cos(yaw / 2);
    const sx = Math.sin(pitch / 2), cx = Math.cos(pitch / 2);
    this.quaternion = [sx * cy, cx * sy, -sx * sy, cx * cy];
  }

  #update = (time: number) => {
    if (!this.#controls) return;
    const delta = Math.min(0.1, (time - this.#last) / 1000);
    this.#last = time;
    let x = Number(this.#keys.has("KeyD")) - Number(this.#keys.has("KeyA"));
    let y = Number(this.#keys.has("Space")) - Number(this.#keys.has("ControlLeft"));
    let z = Number(this.#keys.has("KeyW")) - Number(this.#keys.has("KeyS"));
    if (this.#controls.controller) {
      const pad = navigator.getGamepads?.().find(Boolean);
      if (pad) { x += pad.axes[0] ?? 0; y -= pad.axes[3] ?? 0; z -= pad.axes[1] ?? 0; }
    }
    const speed = (this.#controls.speed ?? 4) * delta;
    const yaw = this.cameraRow()[13];
    this.position[0] += (x * Math.cos(yaw) + z * Math.sin(yaw)) * speed;
    this.position[1] += y * speed;
    this.position[2] += (x * -Math.sin(yaw) + z * Math.cos(yaw)) * speed;
    this.#frame = requestAnimationFrame(this.#update);
  };

  override async dispose() {
    this.detachControls();
    await super.dispose();
  }
}
