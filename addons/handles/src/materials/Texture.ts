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
  id = -1;
  readonly resource: string;
  readonly source?: string | ImageBitmap;
  readonly ready: Promise<void>;
  #disposed = false;

  constructor(scene: Scene, options: TextureOptions = {}) {
    this.scene = scene;
    const resource = `texture-${nextTextureName++}`;
    this.resource = resource;
    this.source = options.source;
    this.ready = (async () => {
      const image =
        typeof options.source === "string"
          ? await createImageBitmap(await (await fetch(options.source)).blob())
          : options.source;
      const registration = scene.registerTexture({
        id: resource,
        source: image,
        size:
          options.size ??
          (image instanceof ImageBitmap
            ? [image.width, image.height, 1]
            : [1, 1, 1]),
        format: options.format ?? "rgba8unorm",
        usage: [
          ...new Set([
            ...(options.usage ?? ["copyDst"]),
            "sampled",
            ...(image instanceof ImageBitmap ? ["render"] : []),
          ]),
        ],
        transient: options.transient ?? false,
      });
      this.id = registration.number;
      await registration.ready;
      if (image instanceof ImageBitmap)
        await scene.core.uploadTexture(resource, image);
    })();
  }

  async dispose() {
    await this.ready;
    if (this.#disposed) return;
    this.#disposed = true;
    await this.scene.core.deleteTexture(this.resource);
    await this.scene.unregisterTexture(this.id);
  }
}
