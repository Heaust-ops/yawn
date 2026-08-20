import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

export type SpotLightOptions = NodeOptions & {
  color?: ArrayLike<number>;
  intensity?: number;
  range?: number;
  innerAngle?: number;
  outerAngle?: number;
};

export class SpotLight extends Node {
  constructor(scene: Scene, options: SpotLightOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      const array = await scene.ensureRows("spotLights", this.id + 1, 32, "f32");
      array.row(this.id).set([
        ...Array.from(options.color ?? [1, 1, 1]), options.intensity ?? 1,
        options.range ?? 10, options.innerAngle ?? 0.35, options.outerAngle ?? 0.7, 1,
      ]);
      return this;
    });
  }

  get innerAngle() { return this.scene.array("spotLights").row(this.id)[5]; }
  set innerAngle(value: number) { this.scene.array("spotLights").row(this.id)[5] = value; }
  get outerAngle() { return this.scene.array("spotLights").row(this.id)[6]; }
  set outerAngle(value: number) { this.scene.array("spotLights").row(this.id)[6] = value; }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.scene.array("spotLights").row(this.id).fill(0);
    await super.dispose();
  }
}
