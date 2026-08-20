import type { Scene } from "../Scene";

export type ShaderMaterialOptions = {
  code: string;
  vertexEntry?: string;
  fragmentEntry?: string;
};

/** User WGSL represented as a material handle; registration rebuilds the Scene graph loadout. */
export class ShaderMaterial {
  readonly scene: Scene;
  id = -1;
  code: string;
  vertexEntry: string;
  fragmentEntry: string;
  readonly ready: Promise<this>;
  #disposed = false;

  constructor(scene: Scene, options: ShaderMaterialOptions) {
    if (!options?.code) throw new TypeError("ShaderMaterial code is required");
    this.scene = scene;
    this.code = options.code;
    this.vertexEntry = options.vertexEntry ?? "vertex";
    this.fragmentEntry = options.fragmentEntry ?? "fragment";
    this.ready = scene.allocateMaterial().then(async (id) => {
      this.id = id;
      await scene.registerShader(this);
      return this;
    });
  }

  async update(options: Partial<ShaderMaterialOptions>) {
    await this.ready;
    if (options.code !== undefined) this.code = options.code;
    if (options.vertexEntry !== undefined) this.vertexEntry = options.vertexEntry;
    if (options.fragmentEntry !== undefined) this.fragmentEntry = options.fragmentEntry;
    await this.scene.registerShader(this);
    return this;
  }

  async dispose() {
    await this.ready;
    if (this.#disposed) return;
    this.#disposed = true;
    await this.scene.unregisterShader(this.id);
    await this.scene.core.deleteObject("materials", this.id);
  }
}
