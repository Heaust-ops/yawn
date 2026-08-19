import { createGraphAst, serializeGraphAst } from "@yawn/render-graph-ast";

/** Converts a plain JavaScript object into the canonical render-graph AST. */
export const graphFromObject = graph => createGraphAst(graph);

/** Serializes a JSO graph and asks Yawn Core to prepare and activate its loadout. */
export function loadGraph(core, graph) {
  if (!core?.loadGraph) throw new TypeError("core must be a YawnCore instance");
  return core.loadGraph(serializeGraphAst(graphFromObject(graph)));
}

export { serializeGraphAst };
