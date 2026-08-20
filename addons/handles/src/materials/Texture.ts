import type { Scene } from "../Scene";

export type TextureOptions = {
  source?: string | ImageBitmap;
  size?: [number | "canvas", number | "canvas", number?];
  format?: string;
  usage?: string[];
  mipmaps?: boolean;
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
      const mipLevelCount =
        image instanceof ImageBitmap && options.mipmaps !== false
          ? Math.floor(Math.log2(Math.max(image.width, image.height))) + 1
          : 1;
      const registration = scene.registerTexture({
        id: resource,
        source: image,
        size:
          options.size ??
          (image instanceof ImageBitmap
            ? [image.width, image.height, 1]
            : [1, 1, 1]),
        format: options.format ?? "rgba8unorm",
        mipLevelCount,
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
      if (image instanceof ImageBitmap) {
        const levels = [image];
        for (let level = 1; level < mipLevelCount; level++)
          levels.push(
            await createImageBitmap(image, {
              resizeWidth: Math.max(1, image.width >> level),
              resizeHeight: Math.max(1, image.height >> level),
              resizeQuality: "high",
            }),
          );
        await Promise.all(
          levels.map((level, index) =>
            scene.core.uploadTexture(resource, level, index),
          ),
        );
      }
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
