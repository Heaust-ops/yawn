export const identityTransform = Object.freeze([
  1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1,
]);

/** Create a conventional instance object from an imported mesh handle. */
export function createInstance(mesh, transform = identityTransform) {
  return mesh.createInstance(transform);
}

/** Frequent mutations stay on the shared-memory path exposed by the handle addon. */
export function updateInstance(instance, transform, typeWords) {
  instance.setTransform(transform);
  instance.setType(typeWords);
}
