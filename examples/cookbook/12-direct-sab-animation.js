/** Write a new model matrix through core's generation-guarded shared SOA column. */
export function animateInstance(core, instance, transform) {
  core.setInstanceTransform(instance.handle, transform);
}

/** Write the opaque 512-bit instance classification used by graph predicates. */
export function classifyInstance(core, instance, words) {
  core.setInstanceType(instance.handle, words);
}
