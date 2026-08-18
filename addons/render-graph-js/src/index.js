import {
  createGraphAst,
  reference,
  serializeGraphAst,
} from "@yawn/render-graph-ast";

/** Compiles a plain JavaScript object description into the canonical graph AST. */
export const graphFromObject = (description) => createGraphAst(description);

/** Small mutable authoring facade; `ast()` returns an immutable canonical AST. */
export class RenderGraph {
  #id;
  #revision;
  #nodes = [];
  #render = [];
  #compute = [];

  constructor(id, revision = 1) {
    this.#id = id;
    this.#revision = revision;
  }

  renderPipeline(declaration) {
    this.#render.push(structuredClone(declaration));
    return this;
  }

  computePipeline(declaration) {
    this.#compute.push(structuredClone(declaration));
    return this;
  }

  node(id, executor, { version = 1, parameters = {}, inputs = {}, state = "enabled" } = {}) {
    this.#nodes.push({
      id,
      state,
      executor: { key: executor, version },
      parameters: structuredClone(parameters),
      inputs: structuredClone(inputs),
    });
    return this;
  }

  ast() {
    return createGraphAst({
      id: this.#id,
      revision: this.#revision,
      pipelines: { render: this.#render, compute: this.#compute },
      nodes: this.#nodes,
    });
  }

  serialize() {
    return serializeGraphAst(this.ast());
  }

  load(core) {
    return core.compileGraph(this.serialize());
  }
}

/** Canonicalizes a JSO/AST and sends its S-expression wire form to Yawn core. */
export function loadGraph(core, description) {
  return core.compileGraph(serializeGraphAst(graphFromObject(description)));
}

export { reference as ref, serializeGraphAst };
