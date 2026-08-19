/** Canonical DAG AST shared by every Yawn render-graph frontend. */
const AST_KIND = "yawn-render-graph";
const AST_VERSION = 1;
const IDENTIFIER = /^[A-Za-z][A-Za-z0-9_.-]*$/;

export class GraphAstError extends TypeError {
  constructor(code, message = code) {
    super(message);
    this.name = "GraphAstError";
    this.code = code;
  }
}

const fail = (code, message) => {
  throw new GraphAstError(code, message);
};
const object = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const identifier = (value) =>
  typeof value === "string" &&
  IDENTIFIER.test(value) &&
  new TextEncoder().encode(value).length <= 64;
const finiteData = (value) =>
  value === null ||
  typeof value === "string" ||
  typeof value === "boolean" ||
  (typeof value === "number" && Number.isFinite(value)) ||
  (Array.isArray(value) && value.every(finiteData)) ||
  (object(value) && Object.values(value).every(finiteData));
const clone = (value) => structuredClone(value);
const freeze = (value) => {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    Object.values(value).forEach(freeze);
  }
  return value;
};
const u32 = (value, name) => {
  if (!Number.isInteger(value) || value < 0 || value > 0xffffffff)
    fail("AST_U32", `${name} must be a uint32`);
  return value;
};

function normalizePipelines(raw = {}) {
  if (!object(raw)) fail("AST_PIPELINES", "pipelines must be an object");
  const render = (raw.render ?? []).map((pipeline) => {
    if (!object(pipeline)) fail("AST_PIPELINE", "render pipeline must be an object");
    const result = {
      name: pipeline.name,
      shader: pipeline.shader,
      vertexEntry: pipeline.vertexEntry ?? "vs_main",
      fragmentEntry: pipeline.fragmentEntry ?? "fs_main",
      doubleSided: pipeline.doubleSided ?? false,
      material: pipeline.material ?? false,
    };
    if (
      !identifier(result.name) ||
      !identifier(result.vertexEntry) ||
      !identifier(result.fragmentEntry) ||
      typeof result.shader !== "string" ||
      typeof result.doubleSided !== "boolean" ||
      typeof result.material !== "boolean"
    )
      fail("AST_PIPELINE", "invalid render pipeline declaration");
    return result;
  });
  const compute = (raw.compute ?? []).map((pipeline) => {
    if (!object(pipeline)) fail("AST_PIPELINE", "compute pipeline must be an object");
    const result = {
      name: pipeline.name,
      shader: pipeline.shader,
      entry: pipeline.entry ?? "main",
      dispatch: Array.from(pipeline.dispatch ?? []),
    };
    if (
      !identifier(result.name) ||
      !identifier(result.entry) ||
      typeof result.shader !== "string" ||
      result.dispatch.length !== 3 ||
      result.dispatch.some((value) => u32(value, "dispatch") === 0)
    )
      fail("AST_PIPELINE", "invalid compute pipeline declaration");
    return result;
  });
  const names = new Set();
  for (const pipeline of [...render, ...compute]) {
    if (names.has(pipeline.name))
      fail("AST_PIPELINE_DUPLICATE", `duplicate pipeline '${pipeline.name}'`);
    names.add(pipeline.name);
  }
  return { render, compute };
}

function normalizeNode(raw) {
  if (!object(raw) || !identifier(raw.id) || !object(raw.executor))
    fail("AST_NODE", "invalid node");
  if (raw.state !== "enabled" && raw.state !== "muted")
    fail("AST_NODE", "node state must be enabled or muted");
  if (!identifier(raw.executor.key)) fail("AST_NODE", "invalid executor key");
  const parameters = clone(raw.parameters ?? {});
  if (!finiteData(parameters)) fail("AST_DATA", "parameters must be finite data");
  if (!object(raw.inputs ?? {})) fail("AST_NODE", "inputs must be an object");
  const inputs = {};
  for (const name of Object.keys(raw.inputs ?? {}).sort()) {
    if (!identifier(name) || !Array.isArray(raw.inputs[name]))
      fail("AST_INPUT", "invalid node input");
    inputs[name] = raw.inputs[name].map((reference) => {
      if (!object(reference) || !identifier(reference.node) || !identifier(reference.socket))
        fail("AST_REFERENCE", "invalid DAG reference");
      return { node: reference.node, socket: reference.socket };
    });
  }
  return {
    id: raw.id,
    state: raw.state,
    executor: { key: raw.executor.key, version: u32(raw.executor.version, "executor version") },
    parameters,
    inputs,
  };
}

/** Creates the canonical in-memory graph AST shared by all authoring frontends. */
export function createGraphAst({ kind, version, id, revision, pipelines = {}, nodes }) {
  if (kind !== undefined && kind !== AST_KIND) fail("AST_KIND", "invalid AST kind");
  if (version !== undefined && version !== AST_VERSION) fail("AST_VERSION", "unsupported AST version");
  if (!identifier(id)) fail("AST_ID", "invalid graph id");
  u32(revision, "revision");
  if (revision === 0) fail("AST_REVISION", "revision must be nonzero");
  if (!Array.isArray(nodes)) fail("AST_NODES", "nodes must be an array");
  const normalized = nodes.map(normalizeNode);
  const ids = new Set();
  for (const node of normalized) {
    if (ids.has(node.id)) fail("AST_NODE_DUPLICATE", `duplicate node '${node.id}'`);
    ids.add(node.id);
  }
  return freeze({
    kind: AST_KIND,
    version: AST_VERSION,
    id,
    revision,
    pipelines: normalizePipelines(pipelines),
    nodes: normalized,
  });
}

export const reference = (node, socket) => {
  if (!identifier(node) || !identifier(socket)) fail("AST_REFERENCE", "invalid DAG reference");
  return Object.freeze({ node, socket });
};

function data(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return String(value);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("AST_DATA", "numbers must be finite");
    return JSON.stringify(value);
  }
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `(array${value.map((item) => ` ${data(item)}`).join("")})`;
  if (object(value))
    return `(object${Object.keys(value)
      .sort()
      .map((key) => ` (field ${JSON.stringify(key)} ${data(value[key])})`)
      .join("")})`;
  fail("AST_DATA", "unsupported AST data value");
}

/** Serializes a graph AST to the only graph wire format accepted by Yawn core. */
export function serializeGraphAst(raw) {
  const graph = createGraphAst(raw);
  const nodes = graph.nodes
    .map((node) => {
      const inputs = Object.entries(node.inputs)
        .map(
          ([name, references]) =>
            `\n        (input ${JSON.stringify(name)}${references
              .map(
                (reference) =>
                  ` (ref ${JSON.stringify(reference.node)} ${JSON.stringify(reference.socket)})`,
              )
              .join("")})`,
        )
        .join("");
      return `\n    (node ${JSON.stringify(node.id)} ${node.state}\n      (executor ${JSON.stringify(node.executor.key)} ${node.executor.version})\n      (params ${data(node.parameters)})\n      (inputs${inputs})\n    )`;
    })
    .join("");
  return `(yawn-graph ${AST_VERSION}\n  (id ${JSON.stringify(graph.id)})\n  (revision ${graph.revision})\n  (pipelines ${data(graph.pipelines)})\n  (nodes${nodes}))\n`;
}

export const GRAPH_AST_KIND = AST_KIND;
export const GRAPH_AST_VERSION = AST_VERSION;
