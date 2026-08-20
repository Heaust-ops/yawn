import type { Scene } from "../Scene";

export type TextureOptions = {
  source?: string | ImageBitmap;
  size?: [number | "canvas", number | "canvas", number?];
  format?: string;
  usage?: string[];
  transient?: boolean;
};

let nextTextureName = 1;

/** A graph texture resource handle; image decoding/upload policy remains outside core. */
export class Texture {
  readonly scene: Scene;
  readonly id: number;
  readonly resource: string;
  readonly source?: string | ImageBitmap;
  readonly ready: Promise<void>;
  #disposed = false;

  constructor(scene: Scene, options: TextureOptions = {}) {
    this.scene = scene;
    this.resource = `texture-${nextTextureName++}`;
    this.source = options.source;
    const registration = scene.registerTexture({
      id: this.resource,
      source: options.source,
      size: options.size ?? [1, 1, 1],
      format: options.format ?? "rgba8unorm",
      usage: [...new Set([...(options.usage ?? ["copyDst"]), "sampled"])],
      transient: options.transient ?? false,
    });
    this.id = registration.number;
    this.ready = registration.ready;
  }

  async dispose() {
    await this.ready;
    if (this.#disposed) return;
    this.#disposed = true;
    await this.scene.unregisterTexture(this.id);
  }
}
