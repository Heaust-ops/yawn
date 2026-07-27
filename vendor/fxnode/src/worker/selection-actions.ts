import type { BatchCommand, Command } from "../commands/types.js";
import type { GraphDocument, LinkId, NodeId } from "../core/types.js";

/** Deterministic removal: selected links not incident to selected nodes, then nodes. */
export function planSelectionRemoval(
  document: GraphDocument,
  selectedNodes: ReadonlySet<NodeId>,
  selectedLinks: ReadonlySet<LinkId>,
): Command | null {
  const nodes = [...selectedNodes].filter((id) => document.nodes[id]).sort();
  const nodeSet = new Set(nodes);
  const links = [...selectedLinks]
    .filter((id) => {
      const link = document.links[id];
      return !!link && !nodeSet.has(link.fromNodeId) && !nodeSet.has(link.toNodeId);
    })
    .sort();
  const commands: BatchCommand[] = [
    ...links.map((id) => ({ type: "link.remove" as const, id })),
    ...nodes.map((id) => ({ type: "node.remove" as const, id })),
  ];
  return { type: "batch", commands };
}

/** Only known standard nodes can be muted. Omitted desired state toggles uniformly (mixed => mute all). */
export function planSelectionMute(
  document: GraphDocument,
  selectedNodes: ReadonlySet<NodeId>,
  isStandard: (id: NodeId) => boolean,
  desired?: boolean,
): Command | null {
  const nodes = [...selectedNodes].filter((id) => document.nodes[id]?.known && isStandard(id)).sort();
  const value = desired ?? nodes.some((id) => !document.nodes[id]!.muted);
  const commands: BatchCommand[] = nodes
    .filter((id) => document.nodes[id]!.muted !== value)
    .map((id) => ({ type: "node.mute", id, value }));
  return { type: "batch", commands };
}
