import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

export type AmbientLightOptions = NodeOptions & { color?: ArrayLike<number>; intensity?: number };

export class AmbientLight extends Node {
  constructor(scene: Scene, options: AmbientLightOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      const array = await scene.ensureRows("ambientLights", this.id + 1, 16, "f32");
      array.row(this.id).set([...Array.from(options.color ?? [1, 1, 1]), options.intensity ?? 0.1]);
      return this;
    });
  }

  get intensity() { return this.scene.array("ambientLights").row(this.id)[3]; }
  set intensity(value: number) { this.scene.array("ambientLights").row(this.id)[3] = value; }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.scene.array("ambientLights").row(this.id).fill(0);
    await super.dispose();
  }
}
