import { Mesh } from "../Mesh";
import type { Scene } from "../Scene";
import { PBRMaterial } from "../materials/PBRMaterial";

let worker: Worker | undefined;
let nextRequest = 1;
const pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();

function importer() {
  if (worker) return worker;
  worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module", name: "yawn-importer" });
  worker.addEventListener("message", ({ data }) => {
    const request = pending.get(data.request);
    if (!request) return;
    pending.delete(data.request);
    if (data.error) request.reject(new Error(data.error));
    else request.resolve(data.result);
  });
  worker.addEventListener("error", () => {
    for (const request of pending.values()) request.reject(new Error("IMPORT_WORKER_ERROR"));
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
  const materials = result.materials.map((options: any) => new PBRMaterial(scene, options));
  await Promise.all(materials.map((material: PBRMaterial) => material.ready));
  const meshes: Mesh[] = [];
  for (const primitive of result.primitives) {
    const mesh = new Mesh(scene, {
      position: primitive.position,
      quaternion: primitive.quaternion,
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
    });
    await mesh.ready;
    meshes.push(mesh);
  }
  return meshes;
}
