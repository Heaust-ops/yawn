import { deepFreeze, nullRecord } from "../core/json.js";
import type { FxNodeCompositionData } from "../composition/types.js";
import type { GraphDocument } from "../core/types.js";
import type { Mutation } from "./mutations.js";

export function reduceMutations<C extends FxNodeCompositionData>(
  document: GraphDocument<C>,
  mutations: readonly Mutation<C>[],
): GraphDocument<C> {
  let nodes = document.nodes;
  let links = document.links;
  for (const mutation of mutations) {
    if (mutation.kind === "document.replaced") return mutation.after;
    if (mutation.kind === "node.set")
      nodes = nullRecord(
        Object.entries(nodes)
          .filter(([key]) => key !== mutation.id)
          .concat(mutation.after ? [[mutation.id, mutation.after]] : []),
      );
    else
      links = nullRecord(
        Object.entries(links)
          .filter(([key]) => key !== mutation.id)
          .concat(mutation.after ? [[mutation.id, mutation.after]] : []),
      );
  }
  return deepFreeze({ ...document, nodes, links });
}
