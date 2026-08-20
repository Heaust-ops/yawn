import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

export type PointLightOptions = NodeOptions & { color?: ArrayLike<number>; intensity?: number; range?: number };

export class PointLight extends Node {
  constructor(scene: Scene, options: PointLightOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      const array = await scene.ensureRows("pointLights", this.id + 1, 32, "f32");
      array.row(this.id).set([...Array.from(options.color ?? [1, 1, 1]), options.intensity ?? 1, options.range ?? 10, 1, 0, 0]);
      return this;
    });
  }

  get color() { return this.scene.array("pointLights").row(this.id).subarray(0, 3); }
  set color(value: ArrayLike<number>) { this.scene.array("pointLights").row(this.id).set(value, 0); }
  get intensity() { return this.scene.array("pointLights").row(this.id)[3]; }
  set intensity(value: number) { this.scene.array("pointLights").row(this.id)[3] = value; }
  get range() { return this.scene.array("pointLights").row(this.id)[4]; }
  set range(value: number) { this.scene.array("pointLights").row(this.id)[4] = value; }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.scene.array("pointLights").row(this.id).fill(0);
    await super.dispose();
  }
}
