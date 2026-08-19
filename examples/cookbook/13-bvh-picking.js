/** Query the optional snapshot/BVH worker and receive wrapped instance handles. */
export function pickNearest(meshHandles, origin, direction) {
  return meshHandles.pickRay(origin, direction, {
    maxDistance: 10_000,
    maxHits: 1,
  });
}
