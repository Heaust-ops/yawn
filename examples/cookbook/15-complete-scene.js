import { culling } from "../render-graph-studio/render-graph/presets.js";
import { importGltf } from "./09-gltf-import-worker.js";
import { compileAndSwitch, dropActiveGraph } from "./08-compile-and-switch.js";

/** Combine the complete JSO graph, shared glTF import, and mesh-handle facade. */
export async function loadCompleteScene(core, gltfUrl) {
  const compiled = await compileAndSwitch(core, culling);
  try {
    const meshes = await importGltf(core, gltfUrl);
    return {
      compiled,
      meshes,
      dispose: () => dropActiveGraph(core, compiled),
    };
  } catch (error) {
    await dropActiveGraph(core, compiled).catch(() => {});
    throw error;
  }
}
