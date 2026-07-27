import { canonicalJsonEqual, deepFreeze, isRecord } from "../core/json.js";
import type { GraphLayoutV2 } from "../core/types.js";
import { bindDocument, type ValidationIssue as BoundValidationIssue } from "./bound-document.js";
import { compileFxNodeComposition } from "./compile.js";
import type { FxNodeCompositionData, FxNodeDefinition, FxNodeValueSchema } from "./types.js";

const token = (s: string) => s.replaceAll("~", "~0").replaceAll("/", "~1");
const equal = (a: unknown, b: unknown) => canonicalJsonEqual(a, b);
const hardSchema = (s: FxNodeValueSchema) =>
  Object.fromEntries(Object.entries(s).filter(([k]) => !["softMin", "softMax", "step", "precision"].includes(k)));
const semanticSocket = (s: FxNodeDefinition["sockets"][string]) => ({
  title: s.title,
  direction: s.direction,
  type: s.type,
  maxIncomingLinks: s.maxIncomingLinks,
  visible: s.visible,
  value: s.value === null ? null : hardSchema(s.value),
});
const semanticNode = (n: FxNodeDefinition) => ({
  version: n.version,
  behavior: n.behavior,
  parameters: Object.fromEntries(Object.entries(n.parameters).map(([k, v]) => [k, hardSchema(v)])),
  sockets: Object.fromEntries(Object.entries(n.sockets).map(([k, v]) => [k, semanticSocket(v)])),
  muteBypass: n.muteBypass,
  migrations: n.migrations,
});
const push = (issues: BoundValidationIssue[], value: BoundValidationIssue) => {
  if (issues.length < 99) issues.push(value);
};
const missing = (
  saved: Record<string, unknown>,
  current: Record<string, unknown>,
  base: string,
  issues: BoundValidationIssue[],
) => {
  for (const id of Object.keys(saved))
    if (!Object.hasOwn(current, id))
      push(issues, {
        code: "composition.definition-missing",
        path: `/composition/${base}/${token(id)}`,
        message: `Current composition is missing saved ${base} definition "${id}"`,
      });
};

/** Checks that current authority conservatively preserves every replay-relevant saved meaning. */
export function saveCompositionCompatibility(
  saved: FxNodeCompositionData,
  current: FxNodeCompositionData,
  baseline: GraphLayoutV2,
): readonly BoundValidationIssue[] {
  const leaves: BoundValidationIssue[] = [];
  if (saved.id !== current.id)
    push(leaves, {
      code: "composition.id",
      path: "/composition/id",
      message: `Saved composition id "${saved.id}" does not match current id "${current.id}"`,
    });
  for (const key of ["socketTypes", "nodeStyles", "resources", "nodes"] as const)
    missing(saved[key], current[key], key, leaves);
  if (!equal(saved.compatibility.wildcardInputTypes, current.compatibility.wildcardInputTypes))
    push(leaves, {
      code: "composition.wildcard",
      path: "/composition/compatibility/wildcardInputTypes",
      message: "Wildcard input types changed",
    });
  for (const id of Object.keys(saved.socketTypes)) {
    const a = saved.socketTypes[id],
      b = current.socketTypes[id];
    if (a && b && !equal(a.acceptsFrom, b.acceptsFrom))
      push(leaves, {
        code: "composition.socket-semantic",
        path: `/composition/socketTypes/${token(id)}/acceptsFrom`,
        message: `Socket type "${id}" compatibility changed`,
      });
  }
  let aDocs: ReturnType<typeof bindDocument> | undefined, bDocs: ReturnType<typeof bindDocument> | undefined;
  try {
    aDocs = bindDocument(compileFxNodeComposition(saved));
    bDocs = bindDocument(compileFxNodeComposition(current));
  } catch {
    /* saved/current are normally compiled/validated by callers */
  }
  for (const id of Object.keys(saved.nodes)) {
    const a = saved.nodes[id],
      b = current.nodes[id];
    if (!a || !b) continue;
    const path = `/composition/nodes/${token(id)}`;
    if (!equal(semanticNode(a), semanticNode(b)))
      push(leaves, { code: "composition.node-semantic", path, message: `Node type "${id}" replay semantics changed` });
    else if (
      aDocs &&
      bDocs &&
      !equal(aDocs.materializeNode("compat-probe", id), bDocs.materializeNode("compat-probe", id))
    )
      push(leaves, {
        code: "composition.node-materialization",
        path,
        message: `Node type "${id}" materializes differently`,
      });
  }
  if (Array.isArray(baseline.nodes))
    for (let i = 0; i < baseline.nodes.length; i++) {
      const node = baseline.nodes[i];
      if (
        isRecord(node) &&
        typeof node.typeId === "string" &&
        !Object.hasOwn(saved.nodes, node.typeId) &&
        Object.hasOwn(current.nodes, node.typeId)
      )
        push(leaves, {
          code: "composition.opaque-promotion",
          path: `/baseline/nodes/${i}/typeId`,
          message: `Opaque baseline node type "${node.typeId}" would be promoted by the current composition`,
        });
    }
  if (!leaves.length) return deepFreeze([]);
  return deepFreeze([
    {
      code: "composition.incompatible",
      path: "/composition",
      message: "Current composition is not a conservative semantic superset of the saved composition",
    },
    ...leaves,
  ]);
}
