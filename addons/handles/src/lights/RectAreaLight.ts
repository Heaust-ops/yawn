import { Node, type NodeOptions } from "../Node";
import type { Scene } from "../Scene";

export type RectAreaLightOptions = NodeOptions & {
  color?: ArrayLike<number>;
  intensity?: number;
  width?: number;
  height?: number;
};

/** Rectangular emitter data for the default clustered forward graph's LTC path. */
export class RectAreaLight extends Node {
  readonly technique = "ltc";

  constructor(scene: Scene, options: RectAreaLightOptions = {}) {
    super(scene, options);
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      const array = await scene.ensureRows("rectAreaLights", this.id + 1, 48, "f32");
      array.row(this.id).set([
        ...Array.from(options.color ?? [1, 1, 1]), options.intensity ?? 1,
        options.width ?? 1, options.height ?? 1, 1, 0,
      ]);
      return this;
    });
  }

  get width() { return this.scene.array("rectAreaLights").row(this.id)[4]; }
  set width(value: number) { this.scene.array("rectAreaLights").row(this.id)[4] = value; }
  get height() { return this.scene.array("rectAreaLights").row(this.id)[5]; }
  set height(value: number) { this.scene.array("rectAreaLights").row(this.id)[5] = value; }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    this.scene.array("rectAreaLights").row(this.id).fill(0);
    await super.dispose();
  }
}
