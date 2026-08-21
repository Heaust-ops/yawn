import { Mesh } from "../Mesh";
import type { Scene } from "../Scene";
import { PBRMaterial } from "../materials/PBRMaterial";
import { Texture } from "../materials/Texture";

let worker: Worker | undefined;
let nextRequest = 1;
const pending = new Map<
  number,
  { resolve: (value: any) => void; reject: (error: Error) => void }
>();

function importer() {
  if (worker) return worker;
  worker = new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
    name: "yawn-importer",
  });
  worker.addEventListener("message", ({ data }) => {
    const request = pending.get(data.request);
    if (!request) return;
    pending.delete(data.request);
    if (data.error) request.reject(new Error(data.error));
    else request.resolve(data.result);
  });
  worker.addEventListener("error", () => {
    for (const request of pending.values())
      request.reject(new Error("IMPORT_WORKER_ERROR"));
    pending.clear();
  });
  return worker;
}

/** Imports glTF/GLB off-thread, then hydrates conventional handles backed by Scene SAB rows. */
export async function importGltf(scene: Scene, url: string | URL) {
  await scene.ready;
  const request = nextRequest++;
  const result = await new Promise<any>((resolve, reject) => {
    pending.set(request, { resolve, reject });
    importer().postMessage({ request, url: String(url) });
  });
  await scene.reserve({
    nodes: result.primitives.length,
    materials: result.materials.length,
  });
  let textures: Texture[] = [];
  const imported = await scene.batchGraphUpdates(async () => {
    const srgb = new Set<number>();
    for (const material of result.materials) {
      if (material.baseColorTexture >= 0) srgb.add(material.baseColorTexture);
      if (material.emissiveTexture >= 0) srgb.add(material.emissiveTexture);
    }
    textures = result.textures.map(
      ({ image }: { image: ImageBitmap }, index: number) =>
        new Texture(scene, {
          source: image,
          format: srgb.has(index) ? "rgba8unorm-srgb" : "rgba8unorm",
        }),
    );
    await Promise.all(textures.map((texture: Texture) => texture.ready));
    const materials = result.materials.map(
      (options: any) =>
        new PBRMaterial(scene, {
          ...options,
          baseColorTexture: textures[options.baseColorTexture],
          metallicRoughnessTexture:
            textures[options.metallicRoughnessTexture],
          normalTexture: textures[options.normalTexture],
          emissiveTexture: textures[options.emissiveTexture],
        }),
    );
    await Promise.all(materials.map((material: PBRMaterial) => material.ready));
    const meshes: Mesh[] = result.primitives.map(
      (primitive: any) =>
        new Mesh(scene, {
          position: primitive.position,
          rotor: primitive.rotor,
          scale: primitive.scale,
          material: materials[primitive.material],
          vertexData: {
            positions: primitive.positions,
            indices: primitive.indices,
            ...(primitive.normals ? { normals: primitive.normals } : {}),
            ...(primitive.tangents ? { tangents: primitive.tangents } : {}),
            ...(primitive.uvs ? { uvs: primitive.uvs } : {}),
            ...(primitive.colors ? { colors: primitive.colors } : {}),
          },
        }),
    );
    await Promise.all(meshes.map((mesh) => mesh.ready));
    return meshes;
  });
  await Promise.all(
    result.textures.map((_: unknown, index: number) =>
      scene.core.deleteTexture(textures[index].resource),
    ),
  );
  return imported;
}
