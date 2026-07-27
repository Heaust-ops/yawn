import type {
  Command,
  CommandError,
  CommandRequest,
  CompatibleFxNodeSaveData,
  FxNodeReplayCommand,
} from "../commands/types.js";
import { decodeFxNodeSaveData } from "../commands/save-data.js";
import { validFxNodeReplayCommand } from "../commands/validate.js";
import { FXNODE_BATCH_LIMIT } from "../commands/limits.js";
import { canonicalJsonEqual, cloneJson, deepFreeze } from "../core/json.js";
import type { CommandId, GraphDocument, GraphLayoutV2, GraphNode, GraphSnapshot, GraphState } from "../core/types.js";
import { invert, type Mutation } from "../engine/mutations.js";
import { reduceMutations } from "../engine/reducer.js";
import {
  bindDocument,
  graphStateFromDocument,
  type DecodeResult as BoundDecodeResult,
  type ValidationIssue as BoundValidationIssue,
} from "./bound-document.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData, FxNodeDefinition } from "./types.js";
import { matchesFxNodeValueSchema } from "./value-matcher.js";

/** Immutable engine state owned by one composition-bound headless runtime. */
export interface EngineState<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly version: number;
  readonly document: GraphDocument<C>;
  readonly undo: readonly { readonly forward: readonly Mutation<C>[]; readonly inverse: readonly Mutation<C>[] }[];
  readonly redo: readonly { readonly forward: readonly Mutation<C>[]; readonly inverse: readonly Mutation<C>[] }[];
  readonly historyLimit: number;
}
/** A committed, versioned mutation batch. */
export interface MutationEnvelope<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly baseVersion: number;
  readonly version: number;
  readonly commandId: CommandId;
  readonly cause: "api" | "gesture" | "undo" | "redo" | "load" | "composition";
  readonly mutations: readonly Mutation<C>[];
}
/** A versioned immutable graph snapshot. */
export interface SnapshotEnvelope<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly version: number;
  readonly snapshot: GraphSnapshot<C>;
}
/** Result of applying a command or state replacement. */
export type TransitionResult<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | {
      readonly status: "committed";
      readonly state: EngineState<C>;
      readonly mutationEnvelope: MutationEnvelope<C>;
      readonly snapshotEnvelope: SnapshotEnvelope<C>;
    }
  | { readonly status: "noop"; readonly state: EngineState<C> }
  | { readonly status: "rejected"; readonly state: EngineState<C>; readonly error: CommandError };
export interface StateReplacementRequest<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly commandId: CommandId;
  readonly expectedVersion: number;
  readonly target: GraphState<C>;
}
/** Result of loading durable graph data. */
export type LoadResult<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | {
      readonly ok: true;
      readonly state: EngineState<C>;
      readonly mutationEnvelope: MutationEnvelope<C>;
      readonly snapshotEnvelope: SnapshotEnvelope<C>;
    }
  | { readonly ok: false; readonly state: EngineState<C>; readonly issues: readonly BoundValidationIssue[] };
export type ReplayResult<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | {
      readonly ok: true;
      readonly status: "committed";
      readonly state: EngineState<C>;
      readonly mutationEnvelope: MutationEnvelope<C>;
      readonly snapshotEnvelope: SnapshotEnvelope<C>;
      readonly saveData: CompatibleFxNodeSaveData<C>;
    }
  | {
      readonly ok: true;
      readonly status: "noop";
      readonly state: EngineState<C>;
      readonly saveData: CompatibleFxNodeSaveData<C>;
    }
  | { readonly ok: false; readonly state: EngineState<C>; readonly issues: readonly BoundValidationIssue[] };
type BoundEngineState<C extends FxNodeCompositionData = FxNodeCompositionData> = EngineState<C>;
type BoundMutationEnvelope<C extends FxNodeCompositionData = FxNodeCompositionData> = MutationEnvelope<C>;
type BoundSnapshotEnvelope<C extends FxNodeCompositionData = FxNodeCompositionData> = SnapshotEnvelope<C>;
type BoundTransitionResult<C extends FxNodeCompositionData = FxNodeCompositionData> = TransitionResult<C>;
type BoundStateReplacementRequest<C extends FxNodeCompositionData = FxNodeCompositionData> = StateReplacementRequest<C>;
type BoundLoadResult<C extends FxNodeCompositionData = FxNodeCompositionData> = LoadResult<C>;
type BoundReplayResult<C extends FxNodeCompositionData = FxNodeCompositionData> = ReplayResult<C>;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b),
  reject = (code: string, message: string): CommandError => ({ code, message });
function snapshotState<C extends FxNodeCompositionData>(state: BoundEngineState<C>): GraphSnapshot<C> {
  return deepFreeze({ version: state.version, ...graphStateFromDocument(state.document) });
}
export interface BoundRebindCodec<C extends FxNodeCompositionData = FxNodeCompositionData> {
  readonly save: (document: GraphDocument<C>) => GraphLayoutV2;
  readonly decodeGraphDocument: (value: unknown) => BoundDecodeResult<C>;
}
export interface BoundAuthorityRebindOptions {
  readonly commandId: CommandId;
  readonly removedNodeTypes?: ReadonlySet<string>;
}
export type BoundAuthorityRebindResult<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | { readonly ok: false; readonly state: BoundEngineState<C>; readonly issues: readonly BoundValidationIssue[] }
  | { readonly ok: true; readonly graphChanged: false; readonly state: BoundEngineState<C> }
  | {
      readonly ok: true;
      readonly graphChanged: true;
      readonly state: BoundEngineState<C>;
      readonly mutationEnvelope: BoundMutationEnvelope<C>;
      readonly snapshotEnvelope: BoundSnapshotEnvelope<C>;
    };
const pointerToken = (value: string) => value.replaceAll("~", "~0").replaceAll("/", "~1");
export function rebindBoundEngineAuthority<C extends FxNodeCompositionData>(
  state: BoundEngineState<C>,
  current: BoundRebindCodec<C>,
  candidate: BoundRebindCodec<C>,
  options: BoundAuthorityRebindOptions,
): BoundAuthorityRebindResult<C> {
  const decoded = candidate.decodeGraphDocument(current.save(state.document));
  if (!decoded.ok) return { ok: false, state, issues: decoded.issues };
  const document = decoded.value,
    demotions: BoundValidationIssue[] = [];
  for (const before of Object.values(state.document.nodes)) {
    if (!before.known) continue;
    const after = document.nodes[before.id];
    const explicitlyRemoved =
      options.removedNodeTypes?.has(before.typeId) === true && after?.known === false && after.typeId === before.typeId;
    if (!after?.known && !explicitlyRemoved && demotions.length < 100)
      demotions.push({
        code: "composition.node-demotion",
        path: `/nodes/${pointerToken(before.id)}/known`,
        message: `Known node type "${before.typeId}" would become unknown`,
      });
  }
  if (demotions.length) return { ok: false, state, issues: deepFreeze(demotions) };
  const graphChanged = !canonicalJsonEqual(state.document, document);
  if (graphChanged && state.version >= Number.MAX_SAFE_INTEGER)
    return {
      ok: false,
      state,
      issues: deepFreeze([{ code: "version.overflow", path: "/", message: "Version exhausted" }]),
    };
  const next: BoundEngineState<C> = deepFreeze({
    version: graphChanged ? state.version + 1 : state.version,
    document,
    undo: [],
    redo: [],
    historyLimit: state.historyLimit,
  });
  if (!graphChanged) return { ok: true, graphChanged: false, state: next };
  const mutationEnvelope: BoundMutationEnvelope<C> = deepFreeze({
    baseVersion: state.version,
    version: next.version,
    commandId: options.commandId,
    cause: "composition",
    mutations: [{ kind: "document.replaced", before: state.document, after: document }],
  });
  return {
    ok: true,
    graphChanged: true,
    state: next,
    mutationEnvelope,
    snapshotEnvelope: deepFreeze({ version: next.version, snapshot: snapshotState(next) }),
  };
}

/** Creates engine operations permanently closed over one compiled composition. */
export function bindEngine<C extends FxNodeCompositionData>(compiled: CompiledFxNodeComposition<C>) {
  const docs = bindDocument(compiled),
    definition = (id: string) =>
      (compiled.nodes as { get(key: string): unknown }).get(id) as FxNodeDefinition | undefined;
  const snapshot = (s: BoundEngineState<C>): GraphSnapshot<C> => snapshotState(s);
  const persisted = (document: GraphDocument<C>) => docs.decodeGraphDocument(docs.save(document));
  const createEngine = (document: GraphDocument<C>, historyLimit = 100): BoundEngineState<C> => {
    if (!Number.isSafeInteger(historyLimit) || historyLimit < 0)
      throw new RangeError("historyLimit must be a finite nonnegative integer");
    const issue = docs.validateDocument(document)[0];
    if (issue) throw new TypeError(`Invalid initial document: ${issue.code}`);
    const closed = persisted(document);
    if (!closed.ok)
      throw new TypeError(`Initial document is not persistable: ${closed.issues[0]?.code ?? "persistence.invalid"}`);
    return deepFreeze({ version: 0, document: closed.value, undo: [], redo: [], historyLimit });
  };
  const plan = (document: GraphDocument<C>, command: Command<C>): readonly Mutation<C>[] | CommandError | null => {
    if (command.type === "batch") {
      if (!command.commands.length) return null;
      if (command.commands.length > FXNODE_BATCH_LIMIT)
        return reject("batch.limit", `Batch exceeds ${FXNODE_BATCH_LIMIT} commands`);
      let draft = document,
        all: Mutation<C>[] = [];
      for (const c of command.commands) {
        const r = plan(draft, c);
        if (r === null) continue;
        if (!Array.isArray(r)) return r as CommandError;
        all.push(...r);
        draft = reduceMutations(draft, r);
      }
      return all.length ? all : null;
    }
    if (command.type === "node.add") {
      if (document.nodes[command.nodeId]) return reject("node.duplicate", "Node id already exists");
      const d = definition(command.nodeType);
      if (!d) return reject("node.type-unknown", "Node type is not declared");
      if (command.parentId) {
        const p = document.nodes[command.parentId];
        if (!p || !p.known || definition(p.typeId)?.behavior !== "frame")
          return reject("parent.frame", "Parent must be an existing known frame");
      }
      return [
        {
          kind: "node.set",
          id: command.nodeId,
          before: null,
          after: docs.materializeNode(command.nodeId, command.nodeType, command.position, command.parentId),
        },
      ];
    }
    if (command.type === "node.remove") {
      const n = document.nodes[command.id];
      if (!n) return reject("node.missing", "Node does not exist");
      return [
        { kind: "node.set", id: n.id, before: n, after: null },
        ...Object.values(document.links)
          .filter((l) => l.fromNodeId === n.id || l.toNodeId === n.id)
          .map((l) => ({ kind: "link.set" as const, id: l.id, before: l, after: null })),
        ...Object.values(document.nodes)
          .filter((x) => x.parentId === n.id)
          .map((x) => {
            const { parentId: _, ...unparented } = x;
            return { kind: "node.set" as const, id: x.id, before: x, after: deepFreeze(unparented) };
          }),
      ];
    }
    if (command.type === "link.add") {
      if (document.links[command.link.id]) return reject("link.duplicate", "Link id already exists");
      if (
        Object.values(document.links).some(
          (l) => l.fromSocketId === command.link.fromSocketId && l.toSocketId === command.link.toSocketId,
        )
      )
        return reject("link.endpoint-duplicate", "Endpoints are already linked");
      return [{ kind: "link.set", id: command.link.id, before: null, after: cloneJson(command.link) }];
    }
    if (command.type === "link.remove") {
      const l = document.links[command.id];
      return l
        ? [{ kind: "link.set", id: l.id, before: l, after: null }]
        : reject("link.missing", "Link does not exist");
    }
    if (command.type === "link.mute") {
      const l = document.links[command.id];
      if (!l) return reject("link.missing", "Link does not exist");
      return l.muted === command.value
        ? null
        : [{ kind: "link.set", id: l.id, before: l, after: deepFreeze({ ...l, muted: command.value }) }];
    }
    if (command.type === "link.replace") {
      const old = document.links[command.removeId];
      if (!old) return reject("link.missing", "Replacement target does not exist");
      if (command.link.toSocketId !== old.toSocketId)
        return reject("link.replace-target", "Replacement must keep target socket");
      if (command.link.id !== old.id && document.links[command.link.id])
        return reject("link.duplicate", "Replacement id collides");
      return [
        { kind: "link.set", id: old.id, before: old, after: null },
        { kind: "link.set", id: command.link.id, before: null, after: cloneJson(command.link) },
      ];
    }
    if (command.type === "undo" || command.type === "redo") return null;
    const node = document.nodes[command.id];
    if (!node) return reject("node.missing", "Node does not exist");
    let after: GraphNode<C>;
    switch (command.type) {
      case "node.move":
        if (same(node.position, command.position)) return null;
        after = { ...node, position: cloneJson(command.position) };
        break;
      case "node.resize":
        if (command.size.x <= 0 || command.size.y <= 0) return reject("size.nonpositive", "Node size must be positive");
        if (same(node.size, command.size)) return null;
        after = { ...node, size: cloneJson(command.size) };
        break;
      case "node.label":
        if (node.label === command.label) return null;
        after = { ...node, label: command.label };
        break;
      case "node.mute":
        if (node.muted === command.value) return null;
        after = { ...node, muted: command.value };
        break;
      case "node.collapse":
        if (node.collapsed === command.value) return null;
        after = { ...node, collapsed: command.value };
        break;
      case "node.parent": {
        if (node.parentId === (command.parentId ?? undefined)) return null;
        const world = (n: GraphNode<C>) => {
          let x = n.position.x,
            y = n.position.y,
            p = n.parentId ? document.nodes[n.parentId] : undefined;
          while (p) {
            x += p.position.x;
            y += p.position.y;
            p = p.parentId ? document.nodes[p.parentId] : undefined;
          }
          return { x, y };
        };
        const p = command.parentId ? document.nodes[command.parentId] : undefined;
        if (command.parentId && (!p || !p.known || definition(p.typeId)?.behavior !== "frame"))
          return reject("parent.frame", "Parent must be an existing known frame");
        const origin = p ? world(p) : { x: 0, y: 0 },
          current = world(node),
          position = { x: current.x - origin.x, y: current.y - origin.y };
        if (command.parentId) after = { ...node, position, parentId: command.parentId };
        else {
          const { parentId: _, ...unparented } = node;
          after = { ...unparented, position };
        }
        break;
      }
      case "node.parameter":
      case "node.parameter-reset": {
        if (!node.known) return reject("node.unknown-readonly", "Unknown node parameters are read-only");
        const schema = definition(node.typeId)?.parameters[command.key];
        if (!schema) return reject("parameter.unknown", "Parameter is not declared");
        const value = command.type === "node.parameter-reset" ? schema.default : command.value;
        if (!matchesFxNodeValueSchema(schema, value))
          return reject("parameter.invalid", "Parameter does not match its schema");
        if (same(node.parameters[command.key], value)) return null;
        after = { ...node, parameters: deepFreeze({ ...node.parameters, [command.key]: cloneJson(value) }) };
        break;
      }
      case "node.socket-default":
      case "node.socket-default-reset": {
        if (!node.known) return reject("node.unknown-readonly", "Unknown node defaults are read-only");
        const index = node.sockets.findIndex((s) => s.id === command.socketId);
        if (index < 0) return reject("socket.missing", "Socket does not exist");
        const key = node.sockets[index]!.key,
          schema = definition(node.typeId)?.sockets[key]?.value,
          value = schema && (command.type === "node.socket-default-reset" ? schema.default : command.value);
        if (!schema || !matchesFxNodeValueSchema(schema, value))
          return reject("socket.default-invalid", "Socket has no editable value schema or value is invalid");
        if (same(node.sockets[index]!.defaultValue, value)) return null;
        after = {
          ...node,
          sockets: node.sockets.map((s, i) => (i === index ? deepFreeze({ ...s, defaultValue: cloneJson(value) }) : s)),
        };
        break;
      }
    }
    return [{ kind: "node.set", id: node.id, before: node, after: deepFreeze(after) }];
  };
  const transition = (state: BoundEngineState<C>, request: CommandRequest<C>): BoundTransitionResult<C> => {
    if (request.expectedVersion !== state.version)
      return { status: "rejected", state, error: reject("version.stale", "Expected version does not match") };
    if (state.version >= Number.MAX_SAFE_INTEGER)
      return { status: "rejected", state, error: reject("version.overflow", "Version exhausted") };
    const cause: BoundMutationEnvelope<C>["cause"] =
      request.command.type === "undo" ? "undo" : request.command.type === "redo" ? "redo" : request.source;
    let mutations: readonly Mutation<C>[],
      undo = state.undo,
      redo = state.redo;
    if (cause === "undo" || cause === "redo") {
      const entry = (cause === "undo" ? state.undo : state.redo).at(-1);
      if (!entry) return { status: "noop", state };
      mutations = cause === "undo" ? entry.inverse : entry.forward;
      undo = cause === "undo" ? state.undo.slice(0, -1) : [...state.undo, entry];
      redo = cause === "redo" ? state.redo.slice(0, -1) : [...state.redo, entry];
    } else {
      const r = plan(state.document, request.command);
      if (r === null) return { status: "noop", state };
      if (!Array.isArray(r)) return { status: "rejected", state, error: r as CommandError };
      mutations = r;
    }
    const document = reduceMutations(state.document, mutations) as GraphDocument<C>;
    const issue = docs.validateDocument(document)[0];
    if (issue)
      return { status: "rejected", state, error: { code: issue.code, message: issue.message, path: issue.path } };
    const closed = persisted(document);
    if (!closed.ok) {
      const persistenceIssue = closed.issues[0];
      return {
        status: "rejected",
        state,
        error: {
          code: persistenceIssue?.code ?? "persistence.invalid",
          message: persistenceIssue?.message ?? "Candidate state is not persistable",
          ...(persistenceIssue?.path === undefined ? {} : { path: persistenceIssue.path }),
        },
      };
    }
    const persistedDocument = canonicalJsonEqual(document, closed.value) ? document : closed.value;
    if (cause === "api" || cause === "gesture") {
      const entry = deepFreeze({ forward: mutations, inverse: invert(mutations) });
      undo = state.historyLimit === 0 ? [] : [...state.undo, entry].slice(-state.historyLimit);
      redo = [];
    }
    const next = deepFreeze({ ...state, version: state.version + 1, document: persistedDocument, undo, redo }),
      mutationEnvelope = deepFreeze({
        baseVersion: state.version,
        version: next.version,
        commandId: request.commandId,
        cause,
        mutations,
      });
    return {
      status: "committed",
      state: next,
      mutationEnvelope,
      snapshotEnvelope: deepFreeze({ version: next.version, snapshot: snapshot(next) }),
    };
  };
  const replaceState = (
    state: BoundEngineState<C>,
    request: BoundStateReplacementRequest<C>,
  ): BoundTransitionResult<C> => {
    if (request.expectedVersion !== state.version)
      return { status: "rejected", state, error: reject("version.stale", "Expected version does not match") };
    const decoded = docs.decodeGraphState(request.target);
    if (!decoded.ok) {
      const issue = decoded.issues[0];
      return {
        status: "rejected",
        state,
        error: {
          code: issue?.code ?? "state.shape",
          message: issue?.message ?? "Invalid GraphState",
          ...(issue?.path === undefined ? {} : { path: issue.path }),
        },
      };
    }
    if (canonicalJsonEqual(state.document, decoded.value)) return { status: "noop", state };
    if (state.version >= Number.MAX_SAFE_INTEGER)
      return { status: "rejected", state, error: reject("version.overflow", "Version exhausted") };
    const mutations = deepFreeze([
        { kind: "document.replaced" as const, before: state.document, after: decoded.value },
      ]),
      entry = deepFreeze({ forward: mutations, inverse: invert(mutations) }),
      undo = state.historyLimit === 0 ? [] : [...state.undo, entry].slice(-state.historyLimit),
      next = deepFreeze({ ...state, version: state.version + 1, document: decoded.value, undo, redo: [] }),
      mutationEnvelope = deepFreeze({
        baseVersion: state.version,
        version: next.version,
        commandId: request.commandId,
        cause: "api" as const,
        mutations,
      });
    return {
      status: "committed",
      state: next,
      mutationEnvelope,
      snapshotEnvelope: deepFreeze({ version: next.version, snapshot: snapshot(next) }),
    };
  };
  const load = (
    state: BoundEngineState<C>,
    value: unknown,
    expectedVersion = state.version,
    id = "load" as CommandId,
  ): BoundLoadResult<C> => {
    if (expectedVersion !== state.version)
      return {
        ok: false,
        state,
        issues: [{ code: "version.stale", path: "/", message: "Expected version does not match" }],
      };
    const decoded = docs.decodeGraphDocument(value);
    if (!decoded.ok) return { ok: false, state, issues: decoded.issues };
    if (state.version >= Number.MAX_SAFE_INTEGER)
      return { ok: false, state, issues: [{ code: "version.overflow", path: "/", message: "Version exhausted" }] };
    const next = deepFreeze({ ...state, version: state.version + 1, document: decoded.value, undo: [], redo: [] }),
      mutationEnvelope = deepFreeze({
        baseVersion: state.version,
        version: next.version,
        commandId: id,
        cause: "load" as const,
        mutations: [{ kind: "document.replaced" as const, before: state.document, after: decoded.value }],
      });
    return {
      ok: true,
      state: next,
      mutationEnvelope,
      snapshotEnvelope: deepFreeze({ version: next.version, snapshot: snapshot(next) }),
    };
  };
  const replaySaveData = (
    state: BoundEngineState<C>,
    value: unknown,
    expectedVersion = state.version,
    id = "load" as CommandId,
  ): BoundReplayResult<C> => {
    if (expectedVersion !== state.version)
      return {
        ok: false,
        state,
        issues: [{ code: "version.stale", path: "/", message: "Expected version does not match" }],
      };
    const decoded = decodeFxNodeSaveData(value, compiled, docs.decodeGraphDocument);
    if (!decoded.ok) return { ok: false, state, issues: decoded.issues };
    let staged: BoundEngineState<C> = deepFreeze({
      version: 0,
      document: decoded.baseline,
      undo: [],
      redo: [],
      historyLimit: state.historyLimit,
    });
    for (let index = 0; index < decoded.value.commands.length; index++) {
      const result = transition(staged, {
        commandId: id,
        expectedVersion: staged.version,
        source: "api",
        command: decoded.value.commands[index]!,
      });
      if (result.status !== "committed")
        return {
          ok: false,
          state,
          issues: [
            result.status === "noop"
              ? { code: "replay.noop", path: `/commands/${index}`, message: "Replay command did not commit" }
              : {
                  code: result.error.code,
                  path: `/commands/${index}${result.error.path ?? ""}`,
                  message: result.error.message,
                },
          ],
        };
      staged = result.state;
    }
    const graphChanged = !canonicalJsonEqual(state.document, staged.document);
    if (graphChanged && state.version >= Number.MAX_SAFE_INTEGER)
      return { ok: false, state, issues: [{ code: "version.overflow", path: "/", message: "Version exhausted" }] };
    const next: BoundEngineState<C> = deepFreeze({
      version: graphChanged ? state.version + 1 : state.version,
      document: graphChanged ? staged.document : state.document,
      undo: staged.undo,
      redo: [],
      historyLimit: state.historyLimit,
    });
    if (!graphChanged) return { ok: true, status: "noop", state: next, saveData: decoded.value };
    const mutationEnvelope: BoundMutationEnvelope<C> = deepFreeze({
      baseVersion: state.version,
      version: next.version,
      commandId: id,
      cause: "load",
      mutations: [{ kind: "document.replaced", before: state.document, after: next.document }],
    });
    return {
      ok: true,
      status: "committed",
      state: next,
      mutationEnvelope,
      snapshotEnvelope: deepFreeze({ version: next.version, snapshot: snapshot(next) }),
      saveData: decoded.value,
    };
  };
  /** Trusted worker-only journal check under this already compiled authority. */
  const validateReplayJournal = (
    baseline: GraphLayoutV2,
    commands: readonly FxNodeReplayCommand<C>[],
    historyLimit = 100,
  ): boolean => {
    const decoded = docs.decodeGraphDocument(baseline);
    if (!decoded.ok) return false;
    let staged: BoundEngineState<C> = deepFreeze({
      version: 0,
      document: decoded.value,
      undo: [],
      redo: [],
      historyLimit,
    });
    for (const command of commands) {
      if (!validFxNodeReplayCommand(command, compiled.nodes)) return false;
      const result = transition(staged, {
        commandId: "journal" as CommandId,
        expectedVersion: staged.version,
        source: "api",
        command,
      });
      if (result.status !== "committed") return false;
      staged = result.state;
    }
    return persisted(staged.document).ok;
  };
  return Object.freeze({
    createEngine,
    transition,
    replaceState,
    load,
    replaySaveData,
    validateReplayJournal,
    getState: snapshot,
  });
}
