import {
  createGraphAst,
  reference,
  serializeGraphAst,
} from "@yawn/render-graph-ast";

const expression = (id, inputs = {}) => ({
  id,
  state: "enabled",
  executor: { key: "and", version: 2 },
  parameters: {},
  inputs,
});

/** Build one DAG whose shared output fans out to two consumers. */
export function canonicalAstExample() {
  const ast = createGraphAst({
    id: "shared_dag",
    revision: 1,
    nodes: [
      expression("source"),
      expression("left", { inputs: [reference("source", "value")] }),
      expression("right", { inputs: [reference("source", "value")] }),
    ],
  });
  return { ast, source: serializeGraphAst(ast) };
}
