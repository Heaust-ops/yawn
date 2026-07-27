import type { FxNodeCompositionData } from "../composition/types.js";
import type { GraphDocument, GraphLink, GraphNode, LinkId, NodeId } from "../core/types.js";

export type Mutation<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | {
      readonly kind: "node.set";
      readonly id: NodeId;
      readonly before: GraphNode<C> | null;
      readonly after: GraphNode<C> | null;
    }
  | {
      readonly kind: "link.set";
      readonly id: LinkId;
      readonly before: GraphLink | null;
      readonly after: GraphLink | null;
    }
  | { readonly kind: "document.replaced"; readonly before: GraphDocument<C>; readonly after: GraphDocument<C> };
export function invert<C extends FxNodeCompositionData>(mutations: readonly Mutation<C>[]): readonly Mutation<C>[] {
  return mutations
    .slice()
    .reverse()
    .map((mutation) =>
      mutation.kind === "node.set"
        ? { kind: "node.set", id: mutation.id, before: mutation.after, after: mutation.before }
        : mutation.kind === "link.set"
          ? { kind: "link.set", id: mutation.id, before: mutation.after, after: mutation.before }
          : { kind: "document.replaced", before: mutation.after, after: mutation.before },
    );
}
