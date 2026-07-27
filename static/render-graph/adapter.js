import {
  CATALOG_VERSION,
  descriptors,
  GRAPH_ID,
  nodeDefinitions,
  socketTypes,
} from "./catalog.js";

export class AuthoringGraphError extends Error {
  constructor(code, details = {}) {
    super(code);
    this.name = "AuthoringGraphError";
    this.code = code;
    this.details = Object.freeze({ ...details });
  }
}

const fail = (code, details) => {
  throw new AuthoringGraphError(code, details);
};
const object = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const identifier = (value) =>
  typeof value === "string" &&
  /^[A-Za-z][A-Za-z0-9_.-]*$/.test(value) &&
  new TextEncoder().encode(value).length <= 64;
const exactKeys = (value, keys) =>
  object(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const finiteJson = (value) =>
  value === null ||
  typeof value === "string" ||
  typeof value === "boolean" ||
  (typeof value === "number" && Number.isFinite(value)) ||
  (Array.isArray(value) && value.every(finiteJson)) ||
  (object(value) && Object.values(value).every(finiteJson));
const canonical = (value) =>
  Array.isArray(value)
    ? value.map(canonical)
    : object(value)
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, canonical(value[key])]),
        )
      : value;
const deepFreeze = (value) => {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) deepFreeze(child);
  }
  return value;
};
const sourceMaps = new WeakMap();
export const getSourceMap = (ir) => sourceMaps.get(ir);
export const mapAuthoringDiagnostic = (ir, diagnostic) => {
  const details = diagnostic?.details;
  const path = [details?.path, diagnostic?.path, details?.field, diagnostic?.field]
    .find((value) => typeof value === "string");
  const map = getSourceMap(ir);
  let match;
  if (path && map)
    for (const key of Object.keys(map))
      if (
        (path === key || path.startsWith(`${key}.`) || path.startsWith(`${key}[`)) &&
        (!match || key.length > match.length)
      )
        match = key;
  return deepFreeze({
    name: diagnostic?.name,
    code: diagnostic?.code,
    message: details?.message ?? diagnostic?.message ?? diagnostic?.code,
    details: details === undefined ? undefined : structuredClone(details),
    path,
    source: match ? structuredClone(map[match]) : undefined,
  });
};

const mapValuePaths = (paths, path, source, value) => {
  paths[path] = source;
  if (Array.isArray(value))
    value.forEach((child, index) => mapValuePaths(paths, `${path}[${index}]`, source, child));
  else if (object(value))
    for (const key of Object.keys(value))
      mapValuePaths(paths, `${path}.${key}`, source, value[key]);
};

function parameterValue(raw, schema, nodeId, key) {
  const expected = schema.type === "json" ? "json" : schema.type;
  if (
    !exactKeys(raw, ["kind", "value"]) ||
    raw.kind !== expected ||
    !finiteJson(raw.value)
  )
    fail("AUTHORING_PARAMETER", { nodeId, parameter: key });
  if (
    (expected === "number" && typeof raw.value !== "number") ||
    (expected === "string" && typeof raw.value !== "string") ||
    (expected === "boolean" && typeof raw.value !== "boolean") ||
    (expected === "json" && !finiteJson(raw.value))
  )
    fail("AUTHORING_PARAMETER", { nodeId, parameter: key });
  return canonical(structuredClone(raw.value));
}

export function adaptFxNodeSnapshot(raw, revision = 1) {
  try {
    const rootKeys = ["graphId", "catalogVersion", "nodes", "links", "metadata", "version"];
    if (
      !exactKeys(raw, rootKeys) ||
      !Array.isArray(raw.nodes) ||
      !Array.isArray(raw.links) ||
      !object(raw.metadata) ||
      !finiteJson(raw.metadata)
    )
      fail("AUTHORING_SHAPE");
    if (raw.graphId !== GRAPH_ID || raw.catalogVersion !== CATALOG_VERSION)
      fail("AUTHORING_CATALOG");
    if (
      !Number.isSafeInteger(raw.version) || raw.version < 0
    )
      fail("AUTHORING_SHAPE");
    if (!Number.isInteger(revision) || revision < 1 || revision > 0xffffffff)
      fail("AUTHORING_REVISION");
    const nodes = new Map(),
      sockets = new Map(),
      paths = {};
    for (let ordinal = 0; ordinal < raw.nodes.length; ordinal++) {
      const n = raw.nodes[ordinal];
      if (!object(n) || !identifier(n.id)) fail("AUTHORING_ID", { id: n?.id });
      if (nodes.has(n.id)) fail("AUTHORING_ID_DUPLICATE", { id: n.id });
      const descriptor = descriptors[n.typeId],
        definition = nodeDefinitions[n.typeId];
      if (!descriptor)
        fail("AUTHORING_NODE_TYPE", { nodeId: n.id, typeId: n.typeId });
      const nodeKeys = ["id", "typeId", "typeVersion", "position", "size", "label", "parameters", "sockets", "muted", "collapsed", "extensions", "known"];
      if (Object.hasOwn(n, "parentId")) nodeKeys.push("parentId");
      if (
        !exactKeys(n, nodeKeys) ||
        n.known !== true ||
        n.typeVersion !== 1 ||
        typeof n.muted !== "boolean" ||
        typeof n.collapsed !== "boolean" ||
        typeof n.label !== "string" ||
        !exactKeys(n.position, ["x", "y"]) || !Number.isFinite(n.position.x) || !Number.isFinite(n.position.y) ||
        !exactKeys(n.size, ["x", "y"]) || !Number.isFinite(n.size.x) || !Number.isFinite(n.size.y) || n.size.x <= 0 || n.size.y <= 0 ||
        (Object.hasOwn(n, "parentId") && !identifier(n.parentId)) ||
        !object(n.extensions) || !finiteJson(n.extensions) ||
        !Array.isArray(n.sockets) ||
        !object(n.parameters)
      )
        fail("AUTHORING_NODE_INVALID", { nodeId: n.id });
      const parameterKeys = Object.keys(definition.parameters);
      if (
        Object.keys(n.parameters).length !== parameterKeys.length ||
        !parameterKeys.every((key) => Object.hasOwn(n.parameters, key))
      )
        fail("AUTHORING_PARAMETER_SET", { nodeId: n.id });
      const parameters = Object.fromEntries(
        parameterKeys.map((key) => [
          key,
          parameterValue(
            n.parameters[key],
            definition.parameters[key],
            n.id,
            key,
          ),
        ]),
      );
      const expected = [
        ...Object.keys(descriptor.inputs),
        ...Object.keys(descriptor.outputs),
      ];
      if (n.sockets.length !== expected.length)
        fail("AUTHORING_SOCKET_SET", { nodeId: n.id });
      for (const s of n.sockets) {
        if (!object(s) || !expected.includes(s.key) || sockets.has(s.id))
          fail("AUTHORING_SOCKET", { nodeId: n.id, socket: s?.key });
        const input = descriptor.inputs[s.key],
          socketDefinition = definition.sockets[s.key],
          direction = input ? "input" : "output",
          dataType = socketDefinition.type,
          socketKeys = ["id", "key", "label", "direction", "dataType", "accepts", "maxIncomingLinks", ...(socketDefinition.value ? ["defaultValue"] : []), "visible"];
        if (
          !exactKeys(s, socketKeys) ||
          s.id !== `${n.id}:${s.key}` ||
          s.label !== socketDefinition.title ||
          s.direction !== direction ||
          s.dataType !== dataType ||
          !Array.isArray(s.accepts) || s.accepts.length !== (direction === "input" ? socketTypes[dataType].acceptsFrom.length : 0) ||
          !s.accepts.every((v, i) => v === (direction === "input" ? socketTypes[dataType].acceptsFrom[i] : undefined)) ||
          (socketDefinition.value
            ? !exactKeys(s.defaultValue, ["kind", "value"]) || !finiteJson(s.defaultValue.value)
            : s.defaultValue !== undefined) ||
          s.visible !== socketDefinition.visible ||
          s.maxIncomingLinks !== socketDefinition.maxIncomingLinks
        )
          fail("AUTHORING_SOCKET", { nodeId: n.id, socket: s.key });
        sockets.set(s.id, {
          node: n.id,
          key: s.key,
          direction,
          semanticType: input
            ? input.accepted.types[0]
            : descriptor.outputs[s.key].type,
          authoringType: s.dataType,
          maxIncomingLinks: s.maxIncomingLinks,
        });
      }
      if (new Set(n.sockets.map((s) => s.key)).size !== expected.length)
        fail("AUTHORING_SOCKET_SET", { nodeId: n.id });
      nodes.set(n.id, {
        ordinal,
        value: {
          id: n.id,
          state: n.muted ? "muted" : "enabled",
          executor: { key: n.typeId, version: 1 },
          parameters,
          inputs: {},
        },
      });
    }
    const incoming = new Map(),
      linkIds = new Set(),
      linkSources = new Map();
    for (let ordinal = 0; ordinal < raw.links.length; ordinal++) {
      const link = raw.links[ordinal];
      if (
        !object(link) ||
        !identifier(link.id) ||
        linkIds.has(link.id) ||
        !exactKeys(link, ["id", "fromNodeId", "fromSocketId", "toNodeId", "toSocketId", "muted", "extensions"]) ||
        typeof link.muted !== "boolean" || !object(link.extensions) || !finiteJson(link.extensions)
      )
        fail("AUTHORING_LINK", { linkId: link?.id });
      linkIds.add(link.id);
      const from = sockets.get(link.fromSocketId),
        to = sockets.get(link.toSocketId);
      if (
        !from ||
        !to ||
        link.fromNodeId !== from.node ||
        link.toNodeId !== to.node ||
        from.direction !== "output" ||
        to.direction !== "input" ||
        (!link.muted && (incoming.get(link.toSocketId) ?? 0) >= to.maxIncomingLinks)
      )
        fail(
          !link.muted && (incoming.get(link.toSocketId) ?? 0) >= (to?.maxIncomingLinks ?? Infinity)
            ? "AUTHORING_LINK_INCOMING"
            : "AUTHORING_LINK",
          !link.muted && (incoming.get(link.toSocketId) ?? 0) >= (to?.maxIncomingLinks ?? Infinity)
            ? { socketId: link.toSocketId }
            : { linkId: link.id },
        );
      const accepted =
        descriptors[nodes.get(to.node).value.executor.key].inputs[to.key]
          .accepted.types;
      const authoringAccepted = socketTypes[nodeDefinitions[nodes.get(to.node).value.executor.key].sockets[to.key].type].acceptsFrom;
      if (!accepted.includes(from.semanticType) || !authoringAccepted.includes(from.authoringType))
        fail("AUTHORING_LINK_TYPE", { linkId: link.id });
      const linkSource = {
        kind: "link",
        linkId: link.id,
        fromNodeId: link.fromNodeId,
        fromSocketId: link.fromSocketId,
        toNodeId: link.toNodeId,
        toSocketId: link.toSocketId,
        muted: link.muted,
        nodeId: to.node,
        input: to.key,
        fromSocket: from.key,
        toSocket: to.key,
      };
      linkSources.set(link.id, linkSource);
      if (!link.muted) {
        incoming.set(link.toSocketId, (incoming.get(link.toSocketId) ?? 0) + 1);
        nodes.get(to.node).value.inputs[to.key] = {
          node: from.node,
          socket: from.key,
        };
      }
    }
    const ordered = [...nodes.values()].sort((a, b) =>
      a.value.id < b.value.id
        ? -1
        : a.value.id > b.value.id
          ? 1
          : a.ordinal - b.ordinal,
    );
    for (let wireOrdinal = 0; wireOrdinal < ordered.length; wireOrdinal++) {
      const item = ordered[wireOrdinal];
      item.value.inputs = Object.fromEntries(
        Object.keys(descriptors[item.value.executor.key].inputs)
          .filter((key) => Object.hasOwn(item.value.inputs, key))
          .map((key) => [key, item.value.inputs[key]]),
      );
      const base = `nodes[${wireOrdinal}]`;
      const nodeSource = { kind: "node", nodeId: item.value.id };
      paths[base] = nodeSource;
      for (const field of ["id", "state", "executor", "executor.key", "executor.version"])
        paths[`${base}.${field}`] = nodeSource;
      paths[`${base}.parameters`] = nodeSource;
      for (const key of Object.keys(item.value.parameters))
        mapValuePaths(paths, `${base}.parameters.${key}`, { kind: "parameter", nodeId: item.value.id, parameter: key }, item.value.parameters[key]);
      for (const key of Object.keys(descriptors[item.value.executor.key].inputs)) {
        const link = raw.links.find((x) => !x.muted && x.toNodeId === item.value.id && sockets.get(x.toSocketId)?.key === key);
        const source = linkSources.get(link?.id) ?? {
          kind: "input",
          nodeId: item.value.id,
          input: key,
          socketId: `${item.value.id}:${key}`,
          unconnected: true,
        };
        paths[`${base}.inputs.${key}`] = source;
        if (link) {
          paths[`${base}.inputs.${key}.node`] = source;
          paths[`${base}.inputs.${key}.socket`] = {
            kind: "socket",
            nodeId: link.fromNodeId,
            socketId: link.fromSocketId,
            socket: sockets.get(link.fromSocketId).key,
            linkId: link.id,
          };
        }
      }
      paths[`${base}.inputs`] = nodeSource;
    }
    const ir = {
      schemaVersion: 2,
      graphId: GRAPH_ID,
      revision,
      nodes: ordered.map((item) => item.value),
    };
    const graphSource = { kind: "graph", graphId: GRAPH_ID };
    for (const field of ["schemaVersion", "graphId", "revision", "nodes"])
      paths[field] = graphSource;
    deepFreeze(paths);
    deepFreeze(ir);
    sourceMaps.set(ir, paths);
    return ir;
  } catch (error) {
    if (error instanceof AuthoringGraphError) throw error;
    fail("AUTHORING_SHAPE");
  }
}
