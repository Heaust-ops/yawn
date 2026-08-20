import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

export type DirectionalLightOptions = NodeOptions & { color?: ArrayLike<number>; intensity?: number };

export class DirectionalLight extends Node {
  constructor(scene: Scene, options: DirectionalLightOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      const array = await scene.ensureRows("directionalLights", this.id + 1, 32, "f32");
      array.row(this.id).set([...Array.from(options.color ?? [1, 1, 1]), options.intensity ?? 1, 1, 0, 0, 0]);
      return this;
    });
  }

  get intensity() { return this.scene.array("directionalLights").row(this.id)[3]; }
  set intensity(value: number) { this.scene.array("directionalLights").row(this.id)[3] = value; }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.scene.array("directionalLights").row(this.id).fill(0);
    await super.dispose();
  }
}
