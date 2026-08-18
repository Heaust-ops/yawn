import { MeshHandles } from "@yawn/mesh-handles";

/** Query the optional snapshot/BVH worker and receive wrapped instance handles. */
export function pickNearest(core, origin, direction) {
  return new MeshHandles(core).pickRay(origin, direction, {
    maxDistance: 10_000,
    maxHits: 1,
  });
}
