import { GltfImporter } from "@yawn/gltf-import";
import { MeshHandles } from "@yawn/mesh-handles";

/** Fetch a glTF URL in the import worker and wrap the resulting protocol handles. */
export async function importGltf(core, url, options) {
  const importer = new GltfImporter(core);
  try {
    const imported = await importer.load(url, options);
    return new MeshHandles(core).fromImportedScene(imported);
  } finally {
    importer.dispose();
  }
}
