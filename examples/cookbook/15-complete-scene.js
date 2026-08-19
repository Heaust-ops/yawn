import { culling } from "../render-graph-studio/render-graph/presets.js";
import { importGltf } from "./09-gltf-import-worker.js";
import { compileAndSwitch } from "./08-compile-and-switch.js";

/** Combine the complete JSO graph, shared glTF import, and mesh-handle facade. */
export async function loadCompleteScene(core, gltfUrl) {
  const meshes = await importGltf(core, gltfUrl);
  const compiled = await compileAndSwitch(core, culling);
  return { compiled, meshes };
}
