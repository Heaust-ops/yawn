import { FXNODE_COMPOSITION_LIMITS } from "../composition/validate.js";
import { isJson } from "../core/json.js";
import type { FxNodeCompositionData, NodeTypeId } from "../composition/types.js";
import type { GraphLink, ParameterValue } from "../core/types.js";
import type { Command, FxNodeReplayCommand } from "./types.js";
import { FXNODE_BATCH_LIMIT } from "./limits.js";

const record = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const exact = (v: Record<string, unknown>, keys: readonly string[]) =>
  Object.keys(v).length === keys.length && keys.every((k) => Object.hasOwn(v, k));
const finitePoint = (v: unknown) =>
  record(v) &&
  exact(v, ["x", "y"]) &&
  typeof v.x === "number" &&
  Number.isFinite(v.x) &&
  typeof v.y === "number" &&
  Number.isFinite(v.y);
const id = (v: unknown) => typeof v === "string" && v.length > 0 && v.length <= 512;
const nodeTypeId = (v: unknown) =>
  typeof v === "string" && v.length > 0 && v.length <= FXNODE_COMPOSITION_LIMITS.maxIdLength;
const parameter = (v: unknown): v is ParameterValue =>
  record(v) &&
  exact(v, ["kind", "value"]) &&
  (v.kind === "number"
    ? typeof v.value === "number" && Number.isFinite(v.value)
    : v.kind === "boolean"
      ? typeof v.value === "boolean"
      : v.kind === "string"
        ? typeof v.value === "string"
        : v.kind === "vector"
          ? Array.isArray(v.value) &&
            v.value.length === 3 &&
            v.value.every((x) => typeof x === "number" && Number.isFinite(x))
          : v.kind === "color"
            ? Array.isArray(v.value) &&
              v.value.length === 4 &&
              v.value.every((x) => typeof x === "number" && Number.isFinite(x))
            : v.kind === "json" && isJson(v.value));
const graphLink = (v: unknown): v is GraphLink =>
  record(v) &&
  exact(v, ["id", "fromNodeId", "fromSocketId", "toNodeId", "toSocketId", "muted", "extensions"]) &&
  id(v.id) &&
  id(v.fromNodeId) &&
  id(v.fromSocketId) &&
  id(v.toNodeId) &&
  id(v.toSocketId) &&
  typeof v.muted === "boolean" &&
  record(v.extensions) &&
  isJson(v.extensions);
function validCommandUnsafe(v: unknown, nested = false): v is Command {
  if (!record(v) || typeof v.type !== "string") return false;
  if (v.type === "batch")
    return (
      !nested &&
      exact(v, ["type", "commands"]) &&
      Array.isArray(v.commands) &&
      v.commands.length <= FXNODE_BATCH_LIMIT &&
      v.commands.every(
        (item) =>
          validCommandUnsafe(item, true) &&
          [
            "node.move",
            "node.resize",
            "node.remove",
            "node.mute",
            "node.collapse",
            "node.parent",
            "link.remove",
            "link.mute",
          ].includes((item as { type: string }).type),
      )
    );
  if (v.type === "undo" || v.type === "redo") return !nested && exact(v, ["type"]);
  if (v.type === "node.remove" || v.type === "link.remove") return exact(v, ["type", "id"]) && id(v.id);
  if (v.type === "node.move" || v.type === "node.resize")
    return (
      exact(v, ["type", "id", v.type === "node.move" ? "position" : "size"]) &&
      id(v.id) &&
      finitePoint(v[v.type === "node.move" ? "position" : "size"])
    );
  if (v.type === "node.mute" || v.type === "node.collapse" || v.type === "link.mute")
    return exact(v, ["type", "id", "value"]) && id(v.id) && typeof v.value === "boolean";
  if (v.type === "node.parent")
    return exact(v, ["type", "id", "parentId"]) && id(v.id) && (v.parentId === null || id(v.parentId));
  if (nested) return false;
  if (v.type === "node.label") return exact(v, ["type", "id", "label"]) && id(v.id) && typeof v.label === "string";
  if (v.type === "node.add")
    return (
      Object.keys(v).every((k) => ["type", "nodeId", "nodeType", "position", "parentId"].includes(k)) &&
      ["type", "nodeId", "nodeType", "position"].every((k) => Object.hasOwn(v, k)) &&
      id(v.nodeId) &&
      id(v.nodeType) &&
      finitePoint(v.position) &&
      (v.parentId === undefined || id(v.parentId))
    );
  if (v.type === "link.add") return exact(v, ["type", "link"]) && record(v.link);
  if (v.type === "link.replace") return exact(v, ["type", "removeId", "link"]) && id(v.removeId) && record(v.link);
  if (v.type === "node.parameter")
    return exact(v, ["type", "id", "key", "value"]) && id(v.id) && typeof v.key === "string" && record(v.value);
  if (v.type === "node.parameter-reset")
    return exact(v, ["type", "id", "key"]) && id(v.id) && typeof v.key === "string";
  if (v.type === "node.socket-default-reset") return exact(v, ["type", "id", "socketId"]) && id(v.id) && id(v.socketId);
  return (
    v.type === "node.socket-default" &&
    exact(v, ["type", "id", "socketId", "value"]) &&
    id(v.id) &&
    id(v.socketId) &&
    record(v.value)
  );
}
/** Total across hostile objects (including proxies with throwing traps). */
export function validCommand(v: unknown): v is Command {
  try {
    return validCommandUnsafe(v);
  } catch {
    return false;
  }
}
export type ReplayNodeTypeAuthority<C extends FxNodeCompositionData> =
  | { readonly has: (id: NodeTypeId<C>) => boolean }
  | ((id: NodeTypeId<C>) => boolean);
/** Total, strict forward-only persisted-command validator. Live wire validation remains deliberately compatible. */
export function validFxNodeReplayCommand<C extends FxNodeCompositionData>(
  v: unknown,
  nodeTypes?: ReplayNodeTypeAuthority<C>,
): v is FxNodeReplayCommand<C> {
  try {
    if (!validCommandUnsafe(v) || v.type === "undo" || v.type === "redo") return false;
    if (v.type === "node.add")
      return (
        nodeTypeId(v.nodeType) &&
        (!nodeTypes ||
          (typeof nodeTypes === "function"
            ? nodeTypes(v.nodeType as NodeTypeId<C>)
            : nodeTypes.has(v.nodeType as NodeTypeId<C>)))
      );
    if (v.type === "link.add" || v.type === "link.replace") return graphLink(v.link);
    if (v.type === "node.parameter" || v.type === "node.socket-default") return parameter(v.value);
    return true;
  } catch {
    return false;
  }
}
