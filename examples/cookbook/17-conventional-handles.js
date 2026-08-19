import { CameraHandle, MaterialHandles, MeshHandles } from "@yawn/mesh-handles";

/** Wrap one import result in the conventional object API without hiding core. */
export function conventionalSceneHandles(core, imported) {
  return {
    camera: new CameraHandle(core),
    meshes: new MeshHandles(core).fromImportedScene(imported),
    materials: new MaterialHandles(core).fromImportedScene(imported),
  };
}

/** Property assignments remain direct SAB writes; no camera/material message is sent. */
export function restyleScene({ camera, materials }) {
  camera.lookAt([4, 3, 6], [0, 0, 0]);
  if (materials[0]) {
    materials[0].baseColor = [0.2, 0.55, 1, 1];
    materials[0].roughness = 0.35;
  }
}
