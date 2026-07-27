import type { GraphDocument, LinkId } from "../core/types.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData } from "../composition/types.js";

/** Derived reroute mute state. Authored flags are never changed. */
export function effectivelyMutedLinks<C extends FxNodeCompositionData>(
  compiled: CompiledFxNodeComposition<C>,
  document: GraphDocument<C>,
): ReadonlySet<LinkId> {
  const muted = new Set<LinkId>(
    Object.values(document.links)
      .filter((l) => l.muted)
      .map((l) => l.id),
  );
  const reroutes = new Set(
    Object.values(document.nodes)
      .filter((n) => n.known && compiled.nodes.get(n.typeId as never)?.behavior === "reroute")
      .map((n) => n.id),
  );
  let changed = true;
  while (changed) {
    changed = false;
    for (const link of Object.values(document.links)) {
      if (muted.has(link.id) || !reroutes.has(link.fromNodeId)) continue;
      const incoming = Object.values(document.links).filter((l) => l.toNodeId === link.fromNodeId);
      if (incoming.length && incoming.every((l) => muted.has(l.id))) {
        muted.add(link.id);
        changed = true;
      }
    }
  }
  return muted;
}
