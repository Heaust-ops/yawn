import { createGraphAst } from "@yawn/render-graph-ast";

/** Exports FXNode nodes with a `pass` payload and links as the canonical pass DAG. */
export function adaptFxNodeSnapshot(snapshot, { pipelines = {}, resources = {} } = {}) {
  if (!Array.isArray(snapshot?.nodes) || !Array.isArray(snapshot?.links)) throw new TypeError("FXNODE_GRAPH");
  const passes = snapshot.nodes.map(node => {
    if (!node?.id || !node.pass) throw new TypeError("FXNODE_PASS");
    return { ...structuredClone(node.pass), id: node.id, after: [...(node.pass.after ?? [])] };
  });
  const byId = new Map(passes.map(pass => [pass.id, pass]));
  for (const link of snapshot.links) {
    const source = link.fromNodeId ?? link.from?.node;
    const target = link.toNodeId ?? link.to?.node;
    if (!byId.has(source) || !byId.has(target)) throw new TypeError("FXNODE_LINK");
    if (!byId.get(target).after.includes(source)) byId.get(target).after.push(source);
  }
  return createGraphAst({ id: snapshot.graphId ?? "fxnode", pipelines, resources, passes });
}
