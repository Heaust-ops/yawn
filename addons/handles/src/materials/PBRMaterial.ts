import type { Scene } from "../Scene";
import type { Texture } from "./Texture";

export type PBRMaterialOptions = {
  baseColor?: ArrayLike<number>;
  metallic?: number;
  roughness?: number;
  emissive?: ArrayLike<number>;
  normalScale?: number;
  alphaCutoff?: number;
  baseColorTexture?: Texture;
  metallicRoughnessTexture?: Texture;
  normalTexture?: Texture;
  emissiveTexture?: Texture;
};

/** Conventional PBR values and texture IDs stored in two shared SOA rows. */
export class PBRMaterial {
  readonly scene: Scene;
  id = -1;
  readonly ready: Promise<this>;
  #disposed = false;

  constructor(scene: Scene, options: PBRMaterialOptions = {}) {
    this.scene = scene;
    const textures = [
      options.baseColorTexture,
      options.metallicRoughnessTexture,
      options.normalTexture,
      options.emissiveTexture,
    ].filter((texture): texture is Texture => texture !== undefined);
    this.ready = Promise.all(textures.map((texture) => texture.ready)).then(async () => {
      const id = await scene.allocateMaterial();
      this.id = id;
      const color = Array.from(options.baseColor ?? [1, 1, 1, 1]);
      const emissive = Array.from(options.emissive ?? [0, 0, 0]);
      if (color.length !== 4 || emissive.length !== 3) throw new TypeError("PBR material vectors");
      scene.array("materials").write(id, [
        ...color,
        options.metallic ?? 0,
        options.roughness ?? 0.7,
        ...emissive,
        options.normalScale ?? 1,
        options.alphaCutoff ?? 0.5,
        0,
      ]);
      scene.array("materialTextures").write(id, [
        (options.baseColorTexture?.id ?? -1) + 1,
        (options.metallicRoughnessTexture?.id ?? -1) + 1,
        (options.normalTexture?.id ?? -1) + 1,
        (options.emissiveTexture?.id ?? -1) + 1,
        0, 0, 0, 0,
      ]);
      return this;
    });
  }

  #values() {
    if (this.id < 0) throw new Error("Await material.ready before reading or writing it");
    return this.scene.array("materials").row(this.id);
  }

  get baseColor() { return this.#values().subarray(0, 4); }
  set baseColor(value: ArrayLike<number>) {
    if (value.length !== 4) throw new RangeError("baseColor");
    this.#values().set(value, 0);
  }
  get metallic() { return this.#values()[4]; }
  set metallic(value: number) { this.#values()[4] = value; }
  get roughness() { return this.#values()[5]; }
  set roughness(value: number) { this.#values()[5] = value; }
  get emissive() { return this.#values().subarray(6, 9); }
  set emissive(value: ArrayLike<number>) {
    if (value.length !== 3) throw new RangeError("emissive");
    this.#values().set(value, 6);
  }

  async dispose() {
    await this.ready;
    if (this.#disposed) return;
    this.#disposed = true;
    await this.scene.core.deleteObject("materials", this.id);
  }
}
