import {
  admitStructuredData,
  canonicalJsonEqual,
  canonicalize,
  cloneJson,
  deepFreeze,
  isJson,
  isRecord,
  nullRecord,
} from "../core/json.js";
import {
  graphId,
  linkId,
  nodeId,
  socketId,
  type GraphDocument,
  type GraphLayoutV2,
  type GraphLink,
  type GraphNode,
  type GraphState,
  type JsonValue,
  type ParameterValue,
  type Socket,
} from "../core/types.js";
import { migrateColorRamp } from "../widgets/color-ramp.js";
import type {
  CompiledFxNodeComposition,
  FxNodeCompositionData,
  FxNodeDefinition,
  FxNodeMigrationStep,
  NodeTypeId,
} from "./types.js";
import { matchesFxNodeValueSchema } from "./value-matcher.js";
import { initialNodeSize } from "../layout/node-dimensions.js";

/** A persistence or graph validation problem, with a stable code and JSON-pointer path. */
export interface ValidationIssue {
  readonly code: string;
  readonly path: string;
  readonly message: string;
}
/** Result of decoding durable graph data under a compiled composition. */
export type DecodeResult<C extends FxNodeCompositionData> =
  | { readonly ok: true; readonly value: GraphDocument<C> }
  | { readonly ok: false; readonly issues: readonly ValidationIssue[] };

type BoundValidationIssue = ValidationIssue;
type BoundDecodeResult<C extends FxNodeCompositionData> = DecodeResult<C>;

export function graphStateFromDocument<C extends FxNodeCompositionData>(document: GraphDocument<C>): GraphState<C> {
  return deepFreeze({
    graphId: document.graphId,
    catalogVersion: document.catalogVersion,
    nodes: Object.values(document.nodes)
      .slice()
      .sort((a, b) => a.id.localeCompare(b.id)),
    links: Object.values(document.links)
      .slice()
      .sort((a, b) => a.id.localeCompare(b.id)),
    metadata: document.metadata,
  });
}
export const PERSISTENCE_LIMITS = Object.freeze({
  maxIssues: 100,
  maxNodes: 10_000,
  maxLinks: 20_000,
  maxSocketsPerNode: 256,
  maxParametersPerNode: 512,
  maxValues: 2_000_000,
  maxStringCodeUnits: 8_388_608,
  maxDepth: 50,
  maxMigrationExecutions: 1_000_000,
});
const finite = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const validString = (v: unknown, max = 512): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= max && !/[\u0000-\u001f\u007f]/u.test(v);
const forbidden = new Set([
  "selection",
  "selected",
  "camera",
  "hover",
  "hovered",
  "runtimeVersion",
  "history",
  "undo",
  "redo",
  "session",
  "layout",
]);
const pointer = (...p: string[]) => `/${p.map((x) => x.replaceAll("~", "~0").replaceAll("/", "~1")).join("/")}`;
const parameter = (v: unknown): v is ParameterValue =>
  isRecord(v) &&
  Object.keys(v).every((k) => k === "kind" || k === "value") &&
  (v.kind === "number"
    ? finite(v.value)
    : v.kind === "boolean"
      ? typeof v.value === "boolean"
      : v.kind === "string"
        ? typeof v.value === "string"
        : v.kind === "vector"
          ? Array.isArray(v.value) && v.value.length === 3 && v.value.every(finite)
          : v.kind === "color"
            ? Array.isArray(v.value) && v.value.length === 4 && v.value.every(finite)
            : v.kind === "json" && isJson(v.value));

const admit = (input: unknown) => admitStructuredData(input, PERSISTENCE_LIMITS);
const STATE_ADMISSION_LIMITS = {
  ...PERSISTENCE_LIMITS,
  maxValues: PERSISTENCE_LIMITS.maxValues + PERSISTENCE_LIMITS.maxNodes + 1,
  maxStringCodeUnits:
    PERSISTENCE_LIMITS.maxStringCodeUnits + PERSISTENCE_LIMITS.maxNodes * "known".length + "version".length,
};
function transient(v: unknown, path: string, out: BoundValidationIssue[], depth = 0): void {
  if (depth > 50 || out.length >= 100) return;
  if (Array.isArray(v)) {
    v.forEach((x, i) => transient(x, `${path}/${i}`, out, depth + 1));
    return;
  }
  if (!isRecord(v)) return;
  for (const [k, x] of Object.entries(v)) {
    if (forbidden.has(k))
      out.push({
        code: "field.transient",
        path: `${path}/${k}`,
        message: "Transient field is forbidden",
      });
    transient(x, `${path}/${k}`, out, depth + 1);
  }
}

/** Creates document operations whose sole definition authority is one compiled composition. */
export function bindDocument<C extends FxNodeCompositionData>(compiled: CompiledFxNodeComposition<C>) {
  const wildcards = new Set<string>(compiled.compatibility.wildcardInputTypes),
    definition = (id: string) =>
      (compiled.nodes as { get(key: string): unknown }).get(id) as
        | (FxNodeDefinition & { readonly typeId: NodeTypeId<C> })
        | undefined;
  const accepts = (s: { direction: string; type: string }) =>
    s.direction === "input"
      ? ((
          compiled.socketTypes as {
            get(key: string): { acceptsFrom: readonly string[] } | undefined;
          }
        ).get(s.type)?.acceptsFrom ?? [])
      : [];
  const socketFor = (id: string, key: string, s: FxNodeDefinition["sockets"][string]): Socket =>
    deepFreeze({
      id: socketId(`${id}:${key}`),
      key,
      label: s.title,
      direction: s.direction,
      dataType: s.type,
      accepts: cloneJson(accepts(s)),
      maxIncomingLinks: s.maxIncomingLinks,
      visible: s.visible,
      ...(s.value ? { defaultValue: cloneJson(s.value.default) } : {}),
    });
  const materializeNode = (
    id: string,
    typeId: NodeTypeId<C>,
    position = { x: 0, y: 0 },
    parentId?: string,
  ): GraphNode<C> => {
    const d = definition(typeId);
    if (!d) throw new TypeError(`Unknown composition type: ${typeId}`);
    const parameters = nullRecord(Object.entries(d.parameters).map(([k, s]) => [k, cloneJson(s.default)]));
    const sockets = Object.entries(d.sockets).map(([k, s]) => socketFor(id, k, s));
    const materialized: Pick<GraphNode, "label" | "parameters" | "sockets"> = { label: d.title, parameters, sockets };
    return deepFreeze({
      id: nodeId(id),
      typeId,
      typeVersion: d.version,
      known: true,
      position: cloneJson(position),
      size: initialNodeSize(d, materialized),
      label: d.title,
      parameters,
      sockets: deepFreeze(sockets),
      muted: false,
      collapsed: false,
      ...(parentId === undefined ? {} : { parentId: nodeId(parentId) }),
      extensions: nullRecord<JsonValue>(),
    }) as GraphNode<C>;
  };
  const exact = (node: GraphNode<C>, d: FxNodeDefinition): boolean => {
    const exactKeys = (value: object, allowed: readonly string[]) => {
      const keys = Object.keys(value);
      return keys.length === allowed.length && keys.every((key) => allowed.includes(key));
    };
    if (
      !exactKeys(node, [
        "id",
        "typeId",
        "typeVersion",
        "known",
        "position",
        "size",
        "label",
        "parameters",
        "sockets",
        "muted",
        "collapsed",
        ...(Object.hasOwn(node, "parentId") ? ["parentId"] : []),
        "extensions",
      ]) ||
      !exactKeys(node.position, ["x", "y"]) ||
      !exactKeys(node.size, ["x", "y"])
    )
      return false;
    const ps = Object.entries(d.parameters);
    if (
      Object.keys(node.parameters).length !== ps.length ||
      !ps.every(([k, s]) => {
        const value = node.parameters[k];
        return isRecord(value) && exactKeys(value, ["kind", "value"]) && matchesFxNodeValueSchema(s, value);
      })
    )
      return false;
    const ss = Object.entries(d.sockets);
    return (
      node.sockets.length === ss.length &&
      ss.every(([key, s], i) => {
        const a = node.sockets[i];
        return (
          !!a &&
          a.id === `${node.id}:${key}` &&
          a.key === key &&
          a.label === s.title &&
          a.direction === s.direction &&
          a.dataType === s.type &&
          a.maxIncomingLinks === s.maxIncomingLinks &&
          a.visible === s.visible &&
          JSON.stringify(a.accepts) === JSON.stringify(accepts(s)) &&
          exactKeys(a, [
            "id",
            "key",
            "label",
            "direction",
            "dataType",
            "accepts",
            "maxIncomingLinks",
            ...(s.value ? ["defaultValue"] : []),
            "visible",
          ]) &&
          (s.value
            ? isRecord(a.defaultValue) &&
              exactKeys(a.defaultValue, ["kind", "value"]) &&
              matchesFxNodeValueSchema(s.value, a.defaultValue)
            : a.defaultValue === undefined)
        );
      })
    );
  };
  const compatible = (
    from: Pick<Socket, "direction" | "dataType">,
    to: Pick<Socket, "direction" | "dataType" | "accepts">,
  ) =>
    from.direction === "output" &&
    to.direction === "input" &&
    (wildcards.has(to.dataType) || to.accepts.includes(from.dataType));
  const validateDocument = (document: GraphDocument<C>): readonly BoundValidationIssue[] => {
    const issues: BoundValidationIssue[] = [],
      incoming = new Map<string, number>();
    for (const l of Object.values(document.links))
      if (!l.muted) incoming.set(l.toSocketId, (incoming.get(l.toSocketId) ?? 0) + 1);
    if (document.catalogVersion !== compiled.version)
      issues.push({
        code: "catalog.version",
        path: "/catalogVersion",
        message: "Document version does not match composition",
      });
    for (const [key, n] of Object.entries(document.nodes)) {
      if (key !== n.id)
        issues.push({
          code: "id.mismatch",
          path: pointer("nodes", key),
          message: "Node key and id differ",
        });
      if (![n.position.x, n.position.y, n.size.x, n.size.y].every(finite) || n.size.x <= 0 || n.size.y <= 0)
        issues.push({
          code: "number.invalid",
          path: pointer("nodes", key),
          message: "Invalid geometry",
        });
      if (n.known) {
        const d = definition(n.typeId);
        if (!d || n.typeVersion !== d.version)
          issues.push({
            code: "catalog.version",
            path: pointer("nodes", key),
            message: "Unsupported definition version",
          });
        else if (!exact(n, d))
          issues.push({
            code: "catalog.invalid",
            path: pointer("nodes", key),
            message: "Known node does not exactly match definition",
          });
      }
      if (n.parentId) {
        const p = document.nodes[n.parentId];
        if (!p || (p.known && definition(p.typeId)?.behavior !== "frame"))
          issues.push({
            code: "parent.frame",
            path: pointer("nodes", key, "parentId"),
            message: "Parent must be a frame",
          });
        const seen = new Set([n.id]);
        let q = p;
        while (q) {
          if (seen.has(q.id)) {
            issues.push({
              code: "parent.cycle",
              path: pointer("nodes", key, "parentId"),
              message: "Parent cycle",
            });
            break;
          }
          seen.add(q.id);
          q = q.parentId ? document.nodes[q.parentId] : undefined;
        }
      }
    }
    for (const [key, l] of Object.entries(document.links)) {
      if (key !== l.id)
        issues.push({
          code: "id.mismatch",
          path: pointer("links", key),
          message: "Link key and id differ",
        });
      const from = document.nodes[l.fromNodeId]?.sockets.find((s) => s.id === l.fromSocketId),
        to = document.nodes[l.toNodeId]?.sockets.find((s) => s.id === l.toSocketId);
      if (!from || !to) {
        issues.push({
          code: "link.endpoint",
          path: pointer("links", key),
          message: "Endpoint does not exist",
        });
        continue;
      }
      if (!compatible(from, to))
        issues.push({
          code: "link.type",
          path: pointer("links", key),
          message: "Incompatible socket endpoints",
        });
      if ((incoming.get(to.id) ?? 0) > to.maxIncomingLinks)
        issues.push({
          code: "link.limit",
          path: pointer("links", key),
          message: "Too many incoming links",
        });
    }
    return deepFreeze(issues.slice(0, 100));
  };
  const decodeAdmittedGraphDocumentUnsafe = (source: unknown): BoundDecodeResult<C> => {
    let input = source;
    const issues: BoundValidationIssue[] = [];
    if (isRecord(input) && Number.isSafeInteger(input.schemaVersion) && Number(input.schemaVersion) > 2)
      return {
        ok: false,
        issues: [
          {
            code: "schema.future",
            path: "/schemaVersion",
            message: "Unsupported future schema version",
          },
        ],
      };
    if (isRecord(input) && input.schemaVersion === 1)
      input = {
        ...input,
        schemaVersion: 2,
        links: Array.isArray(input.links)
          ? input.links.map((x) => (isRecord(x) ? { ...x, muted: false } : x))
          : input.links,
      };
    if (isRecord(input)) for (const [k, v] of Object.entries(input)) if (k !== "nodes") transient(v, `/${k}`, issues);
    if (
      !isRecord(input) ||
      input.schemaVersion !== 2 ||
      !validString(input.graphId) ||
      !Number.isSafeInteger(input.catalogVersion) ||
      Number(input.catalogVersion) <= 0 ||
      !Array.isArray(input.nodes) ||
      !Array.isArray(input.links) ||
      !isRecord(input.metadata) ||
      !isJson(input.metadata)
    )
      return {
        ok: false,
        issues: [...issues, { code: "decode.shape", path: "/", message: "Invalid GraphLayoutV2" }],
      };
    if (input.nodes.length > PERSISTENCE_LIMITS.maxNodes || input.links.length > PERSISTENCE_LIMITS.maxLinks)
      return {
        ok: false,
        issues: [
          {
            code: "limit.collection",
            path: input.nodes.length > PERSISTENCE_LIMITS.maxNodes ? "/nodes" : "/links",
            message: "Document collection limit exceeded",
          },
        ],
      };
    const rawLinks = input.links as unknown[],
      nodes = new Map<string, GraphNode<C>>(),
      staged = new Map<string, { oldId: string; newId: string }[]>();
    let executions = 0;
    for (let i = 0; i < input.nodes.length; i++) {
      const original = input.nodes[i];
      if (
        !isRecord(original) ||
        !validString(original.id) ||
        !validString(original.typeId, 128) ||
        !Number.isSafeInteger(original.typeVersion) ||
        Number(original.typeVersion) <= 0 ||
        !isRecord(original.position) ||
        !finite(original.position.x) ||
        !finite(original.position.y) ||
        !isRecord(original.size) ||
        !finite(original.size.x) ||
        !finite(original.size.y) ||
        Number(original.size.x) <= 0 ||
        Number(original.size.y) <= 0 ||
        typeof original.label !== "string" ||
        original.label.length > 512 ||
        !isRecord(original.parameters) ||
        !isJson(original.parameters) ||
        Object.keys(original.parameters).length > PERSISTENCE_LIMITS.maxParametersPerNode ||
        !Array.isArray(original.sockets) ||
        original.sockets.length > PERSISTENCE_LIMITS.maxSocketsPerNode ||
        typeof original.muted !== "boolean" ||
        typeof original.collapsed !== "boolean" ||
        (original.parentId !== undefined && !validString(original.parentId)) ||
        !isRecord(original.extensions) ||
        !isJson(original.extensions) ||
        Object.hasOwn(original, "known")
      ) {
        issues.push({
          code: "decode.node",
          path: pointer("nodes", String(i)),
          message: "Invalid node",
        });
        continue;
      }
      if (nodes.has(original.id)) {
        issues.push({
          code: "id.duplicate",
          path: pointer("nodes", String(i), "id"),
          message: "Duplicate node id",
        });
        continue;
      }
      const parseSockets = (r: Record<string, unknown>) => {
        const out: Socket[] = [],
          ids = new Set<string>();
        for (const x of r.sockets as unknown[]) {
          if (
            !isRecord(x) ||
            !validString(x.id) ||
            !validString(x.key, 128) ||
            typeof x.label !== "string" ||
            x.label.length > 512 ||
            (x.direction !== "input" && x.direction !== "output") ||
            !validString(x.dataType, 128) ||
            !Array.isArray(x.accepts) ||
            !x.accepts.every((a) => validString(a, 128)) ||
            new Set(x.accepts).size !== x.accepts.length ||
            !Number.isSafeInteger(x.maxIncomingLinks) ||
            Number(x.maxIncomingLinks) < 0 ||
            typeof x.visible !== "boolean" ||
            (x.defaultValue !== undefined && x.defaultValue !== null && !parameter(x.defaultValue)) ||
            (x.metadata !== undefined && (!isRecord(x.metadata) || !isJson(x.metadata))) ||
            ids.has(x.id)
          )
            return;
          ids.add(x.id);
          out.push(x as unknown as Socket);
        }
        return out;
      };
      const originalSockets = parseSockets(original);
      if (!originalSockets) {
        issues.push({
          code: "decode.socket",
          path: pointer("nodes", String(i), "sockets"),
          message: "Invalid socket",
        });
        continue;
      }
      let draft = structuredClone(original),
        rewrites: { oldId: string; newId: string }[] = [],
        soft = false;
      const d = definition(original.typeId);
      if (d && Number(original.typeVersion) < d.version) {
        const byFrom = new Map(d.migrations.map((m) => [m.fromVersion, m]));
        while (Number(draft.typeVersion) < d.version) {
          const edge = byFrom.get(Number(draft.typeVersion));
          if (!edge) {
            soft = true;
            break;
          }
          const next = structuredClone(draft),
            edgeRewrites: { oldId: string; newId: string }[] = [];
          for (const step of edge.steps) {
            if (++executions > PERSISTENCE_LIMITS.maxMigrationExecutions)
              return {
                ok: false,
                issues: [
                  {
                    code: "limit.migrations",
                    path: "/nodes",
                    message: "Migration execution limit exceeded",
                  },
                ],
              };
            if (!run(step, next, d, edgeRewrites)) {
              soft = true;
              break;
            }
          }
          if (soft) break;
          next.typeVersion = edge.toVersion;
          draft = next;
          rewrites.push(...edgeRewrites);
        }
      }
      if (soft) {
        draft = structuredClone(original);
        rewrites = [];
      }
      const sockets = parseSockets(draft);
      if (!sockets) {
        issues.push({
          code: "decode.socket",
          path: pointer("nodes", String(i), "sockets"),
          message: "Invalid socket",
        });
        continue;
      }
      const base = {
        ...draft,
        id: nodeId(String(draft.id)),
        typeId: String(draft.typeId),
        typeVersion: Number(draft.typeVersion),
        sockets,
        ...(typeof draft.parentId === "string" ? { parentId: nodeId(draft.parentId) } : {}),
      };
      let n = cloneJson({ ...base, known: false }) as unknown as GraphNode<C>;
      if (d && !soft && Number(draft.typeVersion) === d.version) {
        transient(draft, pointer("nodes", String(i)), issues);
        const candidate = cloneJson({
          ...base,
          known: true,
          typeId: d.typeId,
        }) as unknown as GraphNode<C>;
        if (exact(candidate, d)) n = candidate;
        else if (Number(original.typeVersion) === d.version) {
          issues.push({
            code: "catalog.invalid",
            path: pointer("nodes", String(i)),
            message: "Known node does not exactly match definition",
          });
          continue;
        } else {
          n = cloneJson({
            ...original,
            sockets: originalSockets,
            known: false,
          }) as unknown as GraphNode<C>;
          rewrites = [];
        }
      } else rewrites = [];
      nodes.set(String(original.id), n);
      if (n.known && rewrites.length) staged.set(n.id, rewrites);
    }
    const linkCandidate = structuredClone(rawLinks);
    for (const r of linkCandidate)
      if (isRecord(r)) {
        for (const rw of staged.get(String(r.fromNodeId)) ?? [])
          if (r.fromSocketId === rw.oldId) r.fromSocketId = rw.newId;
        for (const rw of staged.get(String(r.toNodeId)) ?? []) if (r.toSocketId === rw.oldId) r.toSocketId = rw.newId;
      }
    const links = new Map<string, GraphLink>();
    for (const r of linkCandidate) {
      if (
        !isRecord(r) ||
        !validString(r.id) ||
        !validString(r.fromNodeId) ||
        !validString(r.fromSocketId) ||
        !validString(r.toNodeId) ||
        !validString(r.toSocketId) ||
        typeof r.muted !== "boolean" ||
        !isRecord(r.extensions) ||
        !isJson(r.extensions) ||
        links.has(r.id)
      ) {
        issues.push({
          code: "decode.link",
          path: "/links",
          message: "Invalid link",
        });
        continue;
      }
      links.set(
        r.id,
        cloneJson({
          ...r,
          id: linkId(r.id),
          fromNodeId: nodeId(r.fromNodeId),
          fromSocketId: socketId(r.fromSocketId),
          toNodeId: nodeId(r.toNodeId),
          toSocketId: socketId(r.toSocketId),
        }) as GraphLink,
      );
    }
    if (issues.length) return { ok: false, issues: deepFreeze(issues.slice(0, 100)) };
    const document = deepFreeze({
      schemaVersion: 2 as const,
      graphId: graphId(input.graphId),
      catalogVersion: compiled.version,
      nodes: nullRecord(nodes),
      links: nullRecord(links),
      metadata: cloneJson(input.metadata) as Readonly<Record<string, JsonValue>>,
    });
    const semantic = validateDocument(document);
    return semantic.length ? { ok: false, issues: semantic } : { ok: true, value: document };
    function run(
      step: FxNodeMigrationStep,
      draft: Record<string, unknown>,
      d: FxNodeDefinition,
      rewrites: { oldId: string; newId: string }[],
    ): boolean {
      if (!isRecord(draft.parameters) || !Array.isArray(draft.sockets) || !draft.sockets.every(isRecord)) return false;
      const params = draft.parameters,
        sockets = draft.sockets;
      if (step.kind === "materialize-missing" && step.target === "parameter") {
        const def = d.parameters[step.key];
        if (!def) return false;
        if (!Object.hasOwn(params, step.key)) params[step.key] = structuredClone(def.default);
        return true;
      }
      if (step.kind === "materialize-missing") {
        if (sockets.some((s) => s.key === step.key)) return true;
        const id = `${String(draft.id)}:${step.key}`;
        if (sockets.some((s) => s.id === id)) return false;
        const def = d.sockets[step.key];
        if (!def) return false;
        const ordinal = Object.keys(d.sockets).indexOf(step.key),
          created = structuredClone(socketFor(String(draft.id), step.key, def));
        sockets.splice(Math.min(ordinal, sockets.length), 0, created as unknown as Record<string, unknown>);
        return true;
      }
      if (step.kind === "migrate-parameter") {
        const migrated = migrateColorRamp(params[step.parameter]);
        if (!migrated) return false;
        params[step.parameter] = {
          kind: "json",
          value: structuredClone(migrated),
        };
        return true;
      }
      if (step.kind === "rename-parameter") {
        if (!Object.hasOwn(params, step.from) || Object.hasOwn(params, step.to)) return false;
        params[step.to] = params[step.from];
        delete params[step.from];
        return true;
      }
      const matches = sockets.filter((s) => s.key === step.from);
      if (matches.length !== 1 || sockets.some((s) => s.key === step.to)) return false;
      const old = matches[0]!,
        newId = `${String(draft.id)}:${step.to}`;
      if (sockets.some((s) => s !== old && s.id === newId)) return false;
      for (const l of rawLinks)
        if (
          isRecord(l) &&
          ((l.fromNodeId === draft.id && l.fromSocketId === newId) ||
            (l.toNodeId === draft.id && l.toSocketId === newId))
        )
          return false;
      if (!validString(old.id)) return false;
      const oldId = old.id;
      old.key = step.to;
      old.id = newId;
      rewrites.push({ oldId, newId });
      return true;
    }
  };
  const decodeGraphDocumentUnsafe = (source: unknown): BoundDecodeResult<C> => {
    const admitted = admit(source);
    return admitted.ok
      ? decodeAdmittedGraphDocumentUnsafe(admitted.value)
      : { ok: false, issues: deepFreeze(admitted.issues) };
  };
  const decodeGraphDocument = (source: unknown): BoundDecodeResult<C> => {
    try {
      return decodeGraphDocumentUnsafe(source);
    } catch {
      return {
        ok: false,
        issues: [
          {
            code: "decode.failure",
            path: "/",
            message: "Document could not be decoded",
          },
        ],
      };
    }
  };
  const decodeGraphState = (source: unknown): BoundDecodeResult<C> => {
    try {
      const admitted = admitStructuredData(source, STATE_ADMISSION_LIMITS);
      if (!admitted.ok) return { ok: false, issues: deepFreeze(admitted.issues) };
      const input = admitted.value;
      const shape = () => ({
        ok: false as const,
        issues: deepFreeze([{ code: "state.shape", path: "/", message: "Invalid GraphState" }]),
      });
      if (!isRecord(input)) return shape();
      const keys = Object.keys(input),
        allowed = ["version", "graphId", "catalogVersion", "nodes", "links", "metadata"];
      if (
        !["graphId", "catalogVersion", "nodes", "links", "metadata"].every((k) => Object.hasOwn(input, k)) ||
        keys.some((k) => !allowed.includes(k)) ||
        !validString(input.graphId) ||
        !Number.isSafeInteger(input.catalogVersion) ||
        Number(input.catalogVersion) <= 0 ||
        !Array.isArray(input.nodes) ||
        !Array.isArray(input.links) ||
        !isRecord(input.metadata) ||
        !isJson(input.metadata)
      )
        return shape();
      if (input.nodes.length > PERSISTENCE_LIMITS.maxNodes)
        return {
          ok: false,
          issues: deepFreeze([{ code: "limit.nodes", path: "/nodes", message: "Node limit exceeded" }]),
        };
      if (input.links.length > PERSISTENCE_LIMITS.maxLinks)
        return {
          ok: false,
          issues: deepFreeze([{ code: "limit.links", path: "/links", message: "Link limit exceeded" }]),
        };
      const hasVersion = Object.hasOwn(input, "version"),
        durableValues = admitted.metrics.values - input.nodes.length - (hasVersion ? 1 : 0) + 1,
        durableStrings =
          admitted.metrics.stringCodeUnits -
          input.nodes.length * "known".length -
          (hasVersion ? "version".length : 0) +
          "schemaVersion".length;
      if (durableValues > PERSISTENCE_LIMITS.maxValues)
        return {
          ok: false,
          issues: deepFreeze([{ code: "limit.values", path: "/", message: "Document exceeds inspected value limit" }]),
        };
      if (durableStrings > PERSISTENCE_LIMITS.maxStringCodeUnits)
        return {
          ok: false,
          issues: deepFreeze([{ code: "limit.strings", path: "/", message: "Document exceeds string limit" }]),
        };
      const nodes: unknown[] = [];
      for (const node of input.nodes) {
        if (!isRecord(node) || !Object.hasOwn(node, "known") || typeof node.known !== "boolean")
          return {
            ok: false,
            issues: deepFreeze([
              { code: "state.inexact", path: "/nodes", message: "GraphState is not exact for this composition" },
            ]),
          };
        const { known: _, ...durable } = node;
        nodes.push(durable);
      }
      const requested = {
        graphId: input.graphId,
        catalogVersion: input.catalogVersion,
        nodes: input.nodes,
        links: input.links,
        metadata: input.metadata,
      };
      const decoded = decodeAdmittedGraphDocumentUnsafe({
        schemaVersion: 2,
        graphId: input.graphId,
        catalogVersion: input.catalogVersion,
        nodes,
        links: input.links,
        metadata: input.metadata,
      });
      if (!decoded.ok) return decoded;
      return canonicalJsonEqual(requested, graphStateFromDocument(decoded.value))
        ? decoded
        : {
            ok: false,
            issues: deepFreeze([
              { code: "state.inexact", path: "/", message: "GraphState is not exact for this composition" },
            ]),
          };
    } catch {
      return { ok: false, issues: [{ code: "state.shape", path: "/", message: "Invalid GraphState" }] };
    }
  };
  const save = (document: GraphDocument<C>): GraphLayoutV2 =>
    deepFreeze({
      schemaVersion: 2,
      graphId: document.graphId,
      catalogVersion: document.catalogVersion,
      nodes: Object.values(document.nodes)
        .map(({ known: _, parentId, ...node }) => ({ ...node, ...(parentId === undefined ? {} : { parentId }) }))
        .sort((a, b) => a.id.localeCompare(b.id)),
      links: Object.values(document.links)
        .slice()
        .sort((a, b) => a.id.localeCompare(b.id)),
      metadata: document.metadata,
    });
  const serializeGraphDocument = (d: GraphDocument<C>) => JSON.stringify(canonicalize(save(d)));
  const parseGraphDocument = (text: string): BoundDecodeResult<C> => {
    try {
      return decodeGraphDocument(JSON.parse(text));
    } catch {
      return {
        ok: false,
        issues: [{ code: "decode.json", path: "/", message: "Invalid JSON" }],
      };
    }
  };
  return Object.freeze({
    emptyDocument: (id = "graph"): GraphDocument<C> =>
      deepFreeze({
        schemaVersion: 2,
        graphId: graphId(id),
        catalogVersion: compiled.version,
        nodes: nullRecord(),
        links: nullRecord(),
        metadata: nullRecord(),
      }),
    materializeNode,
    validateDocument,
    decodeGraphDocument,
    decodeGraphState,
    parseGraphDocument,
    save,
    serializeGraphDocument,
    socketsCompatible: compatible,
  });
}
