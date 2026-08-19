export const GRAPH_AST_VERSION = 1;

const data = value => value === null || typeof value === "string" || typeof value === "boolean" ||
  (typeof value === "number" && Number.isFinite(value)) ||
  (Array.isArray(value) && value.every(data)) ||
  (value?.constructor === Object && Object.values(value).every(data));

function freeze(value) {
  if (value && typeof value === "object") {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

/** Creates the data-only AST consumed by every graph frontend. DAG edges are pass `after` IDs. */
export function createGraphAst(graph) {
  if (!data(graph) || graph?.constructor !== Object || typeof graph.id !== "string" ||
      !Array.isArray(graph.passes)) throw new TypeError("GRAPH_AST");
  return freeze(structuredClone(graph));
}

function encode(value) {
  if (value === null || typeof value === "boolean" || typeof value === "number") return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `(array${value.map(item => ` ${encode(item)}`).join("")})`;
  return `(object${Object.keys(value).sort().map(key =>
    ` (field ${JSON.stringify(key)} ${encode(value[key])})`).join("")})`;
}

/** Serializes the AST as an S-expression; named pass references preserve DAG fan-out. */
export function serializeGraphAst(graph) {
  return `(yawn-graph ${GRAPH_AST_VERSION} ${encode(createGraphAst(graph))})`;
}
