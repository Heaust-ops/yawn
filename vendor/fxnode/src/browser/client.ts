import type { Command, FxNodeSaveData } from "../commands/types.js";
import type { GraphLayoutV2, GraphSnapshot } from "../core/types.js";
import type { MutationEnvelope, SnapshotEnvelope } from "../composition/bound-engine.js";
import { createInitialFxNodeComposition } from "../composition/compile.js";
import {
  validCommandReceipt,
  validCompositionReceipt,
  validWorkerMessage,
  PROTOCOL_VERSION,
  type CompositionChange,
  type CompositionChangeEnvelope,
  type CompositionReceipt,
  type CompositionRevisionExpectation,
  type CompositionUpdateWire,
  type InputEventWire,
  type PointerFence,
  type VersionExpectation,
  type WorkerRequest,
} from "./protocol.js";
import {
  advancePointerLaneFence,
  createPointerLane,
  pointerLaneFence,
  publishPointerMove,
  supportsPointerLane,
  type PointerLaneSnapshot,
  type PointerMoveWire,
} from "./pointer-lane.js";
import type {
  FxNodeCompositionData,
  NodeTypeId,
  FxNodeDefinition,
  FxNodeSocketTypeDefinition,
  FxNodeStyleDefinition,
  FxNodeTheme,
} from "../composition/types.js";
import type { NodeReferenceCheck, ReferenceCheck } from "../composition/references.js";
import defaultWorkerUrl from "../worker/fxnode.worker.ts?worker&url";
import type {
  AddNodeParams,
  FxNodeActionOptions,
  FxNodeHostRequest,
  FxNodeHostSnapshot,
  FxNodeCamera,
  FxNodeInput,
  FxNodeResourceAuthorization,
  FxNodeResourceData,
  FxNodeSelectionSnapshot,
  FxNodeViewport,
} from "./host-types.js";
import {
  decodeFxNodeActionOptions,
  decodeFxNodeAddNodeParams,
  decodeFxNodeCamera,
  decodeFxNodeInput,
  decodeFxNodeResourceAuthorization,
  decodeFxNodeResourceData,
  decodeFxNodeViewport,
} from "./host-decode.js";
import { fxNodeDevicePixels, FXNODE_VIEW_LIMITS } from "./view-limits.js";

export interface FxNodeIssue {
  readonly code: string;
  readonly message: string;
  readonly path?: string;
}
export class FxNodeCapabilityError extends Error {
  override name = "FxNodeCapabilityError";
  constructor(
    message: string,
    readonly code = "capability.unavailable",
  ) {
    super(message);
  }
}
export class FxNodeProtocolError extends Error {
  override name = "FxNodeProtocolError";
  constructor(message?: string, options?: ErrorOptions) {
    super(message, options);
  }
}
export class FxNodeWorkerError extends Error {
  override name = "FxNodeWorkerError";
  constructor(
    message: string,
    readonly code = "worker.error",
    readonly issues?: readonly FxNodeIssue[],
    readonly path?: string,
  ) {
    super(message);
  }
}
export class FxNodeDestroyedError extends Error {
  override name = "FxNodeDestroyedError";
  constructor() {
    super("FxNode has been destroyed");
  }
}
export class FxNodeViewDetachedError extends Error {
  override name = "FxNodeViewDetachedError";
  constructor() {
    super("FxNode view has been detached");
  }
}

export type CommandIntent =
  | Exclude<Command, { type: "node.add" }>
  | (Omit<Extract<Command, { type: "node.add" }>, "nodeId"> & {
      nodeId?: Extract<Command, { type: "node.add" }>["nodeId"];
    })
  | (Omit<Extract<Command, { type: "link.add" }>, "link"> & {
      link: Omit<Extract<Command, { type: "link.add" }>["link"], "id"> & {
        id?: Extract<Command, { type: "link.add" }>["link"]["id"];
      };
    });
export interface FxNode {
  attachView(options: FxNodeViewOptions): Promise<FxNodeView>;
  dispatch(intent: CommandIntent, options?: { expectedVersion?: number }): Promise<CommandReceipt>;
  undo(options?: { expectedVersion?: number }): Promise<CommandReceipt>;
  redo(options?: { expectedVersion?: number }): Promise<CommandReceipt>;
  save(): Promise<GraphLayoutV2>;
  getSaveData(): Promise<FxNodeSaveData<FxNodeCompositionData>>;
  load(data: unknown, options?: { expectedVersion?: number }): Promise<CommandReceipt>;
  getState(): Promise<GraphSnapshot<FxNodeCompositionData>>;
  setState(state: unknown, options?: { expectedVersion?: number }): Promise<CommandReceipt>;
  onMutations(callback: (event: MutationEnvelope<FxNodeCompositionData>) => void): () => void;
  onSnapshots(callback: (event: SnapshotEnvelope<FxNodeCompositionData>) => void): () => void;
  setTheme(theme: FxNodeTheme, options?: CompositionUpdateOptions): Promise<CompositionReceipt>;
  setHeaderStyles(
    styles: Readonly<Record<string, FxNodeStyleDefinition>>,
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt>;
  setCompatibility(
    compatibility: FxNodeCompositionData["compatibility"],
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt>;
  composeSocket(
    id: string,
    definition: FxNodeSocketTypeDefinition,
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt>;
  removeSocket(id: string, options?: CompositionUpdateOptions): Promise<CompositionReceipt>;
  composeNode<const D extends FxNodeDefinition>(
    id: string,
    definition: D & NodeReferenceCheck<FxNodeCompositionData, D>,
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt>;
  removeNode(id: string, options?: CompositionUpdateOptions): Promise<CompositionReceipt>;
  loadComposition<const C extends FxNodeCompositionData>(
    composition: C & ReferenceCheck<C>,
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt>;
  onCompositionChanges(callback: (event: CompositionChangeEnvelope) => void): () => void;
  destroy(): void;
}
export interface FxNodeViewOptions {
  canvas: HTMLCanvasElement;
  viewport: FxNodeViewport;
  initialCamera?: FxNodeCamera;
}
export interface FxNodeView {
  readonly id: string;
  feedInput: (input: FxNodeInput) => void;
  setViewport: (viewport: FxNodeViewport) => Promise<void>;
  getHostSnapshot: () => FxNodeHostSnapshot;
  subscribeHost: (callback: () => void) => () => void;
  addNode(params: AddNodeParams, options?: FxNodeActionOptions): Promise<CommandReceipt>;
  removeSelected(options?: FxNodeActionOptions): Promise<CommandReceipt>;
  setSelectedMuted(value: boolean, options?: FxNodeActionOptions): Promise<CommandReceipt>;
  onHostRequests(callback: (request: FxNodeHostRequest) => void): () => void;
  provideResource(
    authorization: FxNodeResourceAuthorization,
    data: FxNodeResourceData,
    options?: FxNodeActionOptions,
  ): Promise<CommandReceipt>;
  whenRendered(): Promise<void>;
  detach(): Promise<void>;
}
export interface CreateFxNodeOptions {
  applicationId: string;
  applicationVersion: number;
  resources: FxNodeCompositionData["resources"];
  historyLimit?: number;
  workerUrl?: string | URL;
}
export interface CommandReceipt {
  status: "committed" | "noop";
  version: number;
}
export interface CompositionUpdateOptions {
  readonly expectedRevision?: number;
}
export type { CompositionChange, CompositionChangeEnvelope, CompositionReceipt } from "./protocol.js";

type Pending = {
  requestType: RpcRequest["type"];
  owner: "root" | FxNodeViewClient;
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
};
type Barrier = { resolve: () => void; reject: (reason: unknown) => void };
type RpcRequest = WorkerRequest extends infer R
  ? R extends { id: string }
    ? Omit<R, "id" | "protocol">
    : never
  : never;
const STARTUP_TIMEOUT_MS = 5_000;
const emptySelection = (): FxNodeSelectionSnapshot =>
  Object.freeze({ nodeCount: 0, linkCount: 0, canRemove: false, mute: Object.freeze({ enabled: false as const }) });

const reservedCanvases = new WeakSet<HTMLCanvasElement>();

class FxNodeClient implements FxNode {
  private terminalError: Error | undefined;
  private readonly pending = new Map<string, Pending>();
  private readonly views = new Map<string, FxNodeViewClient>();
  private readonly mutations = new Set<(event: MutationEnvelope<FxNodeCompositionData>) => void>();
  private readonly snapshots = new Set<(event: SnapshotEnvelope<FxNodeCompositionData>) => void>();
  private readonly compositionChanges = new Set<(event: CompositionChangeEnvelope) => void>();
  private readonly unclaimedCompositionEvents = new Map<number, CompositionChangeEnvelope>();
  private compositionRevision: number | undefined;
  private aggregateDevicePixels = 0;
  private attachingViewCount = 0;
  private readonly usedViewIds = new Set<string>();
  private rpcCounter = 0;
  constructor(private readonly worker: Worker) {
    worker.onmessage = this.onMessage;
    worker.onerror = () =>
      this.shutdown(
        new FxNodeCapabilityError(
          "The FxNode module worker failed to load. Check workerUrl, CSP worker-src, URL accessibility, and JavaScript MIME type.",
          "worker.load",
        ),
      );
    worker.onmessageerror = () =>
      this.shutdown(new FxNodeProtocolError("The FxNode worker sent an uncloneable message"));
  }

  async initialize(
    applicationId: string,
    applicationVersion: number,
    resources: FxNodeCompositionData["resources"],
    historyLimit: number,
  ): Promise<void> {
    const timeout = setTimeout(
      () =>
        this.shutdown(
          new FxNodeCapabilityError(
            "FxNode worker startup timed out. Check workerUrl, CSP worker-src, URL accessibility, and JavaScript MIME type.",
            "worker.timeout",
          ),
        ),
      STARTUP_TIMEOUT_MS,
    );
    try {
      await this.post({
        type: "init",
        applicationId,
        applicationVersion,
        resources,
        historyLimit,
      });
      if (this.compositionRevision !== 0) this.protocolFailure("FxNode worker did not publish its initial state");
    } finally {
      clearTimeout(timeout);
    }
  }

  private readonly onMessage = (event: MessageEvent<unknown>): void => {
    let recognizableBitmap: ImageBitmap | undefined;
    try {
      recognizableBitmap = this.bitmapFrom(event.data);
      if (!validWorkerMessage(event.data)) {
        this.shutdown(new FxNodeProtocolError("Invalid message from FxNode worker"));
        return;
      }
      const data = event.data;
      if (data.type === "response") {
        const pending = this.pending.get(data.id);
        if (!pending) return;
        if (
          (pending.requestType === "init" ||
            pending.requestType === "view.attach" ||
            pending.requestType === "view.detach" ||
            pending.requestType === "view.viewport") &&
          data.ok &&
          Object.hasOwn(data, "value")
        ) {
          this.shutdown(new FxNodeProtocolError("FxNode worker returned a value for a valueless response"));
          return;
        }
        this.pending.delete(data.id);
        if (data.ok) {
          if (pending.requestType === "init") this.compositionRevision = 0;
          pending.resolve(Object.hasOwn(data, "value") ? structuredClone(data.value) : undefined);
        } else {
          const capabilityCodes = new Set([
            "atlas.create",
            "atlas.context",
            "atlas.crop",
            "atlas.context-lost",
            "view.render.unavailable",
          ]);
          pending.reject(
            capabilityCodes.has(data.error.code)
              ? new FxNodeCapabilityError(data.error.message, data.error.code)
              : new FxNodeWorkerError(
                  data.error.message,
                  data.error.code,
                  data.error.issues as readonly FxNodeIssue[] | undefined,
                  data.error.path,
                ),
          );
        }
      } else if (data.type === "composition.event") {
        const envelope = data.envelope;
        if (
          this.compositionRevision === undefined ||
          envelope.baseRevision !== this.compositionRevision ||
          envelope.revision !== this.compositionRevision + 1 ||
          this.unclaimedCompositionEvents.has(envelope.revision)
        ) {
          this.shutdown(new FxNodeProtocolError("FxNode worker published an invalid composition revision sequence"));
          return;
        }
        this.compositionRevision = envelope.revision;
        for (const view of this.views.values()) view.clearDocumentDependentHostState(envelope.revision);
        this.unclaimedCompositionEvents.set(envelope.revision, envelope);
        this.notify(this.compositionChanges, envelope);
      } else if (data.type === "mutation") {
        for (const view of this.views.values()) view.clearDocumentDependentHostState();
        this.notify(this.mutations, data.envelope);
      } else if (data.type === "snapshot.event") this.notify(this.snapshots, data.envelope);
      else if (data.type === "fatal") this.shutdown(new FxNodeWorkerError(data.error.message, data.error.code));
      else this.views.get(data.viewId)?.consumeMessage(data);
    } finally {
      try {
        recognizableBitmap?.close();
      } catch {}
    }
  };

  private bitmapFrom(data: unknown): ImageBitmap | undefined {
    if (typeof ImageBitmap === "undefined" || typeof data !== "object" || data === null) return undefined;
    try {
      const bitmap = (data as { bitmap?: unknown }).bitmap;
      return bitmap instanceof ImageBitmap ? bitmap : undefined;
    } catch {
      return undefined;
    }
  }
  private notify<T>(callbacks: ReadonlySet<(event: T) => void>, event: T): void {
    for (const callback of callbacks)
      try {
        callback(event);
      } catch (error) {
        console.error("FxNode subscriber failed", error);
      }
  }
  safePost(message: WorkerRequest, transfer: Transferable[] = []): boolean {
    try {
      this.worker.postMessage(message, transfer);
      return true;
    } catch {
      return false;
    }
  }
  requiredPost(message: WorkerRequest): boolean {
    if (this.safePost(message)) return true;
    this.shutdown(new FxNodeProtocolError("Unable to send a message to the FxNode worker"));
    return false;
  }
  post<T>(
    message: RpcRequest,
    transfer: Transferable[] = [],
    onPosted?: () => void,
    owner: "root" | FxNodeViewClient = "root",
  ): Promise<T> {
    if (this.terminalError) return Promise.reject(this.terminalError);
    if (
      (message.type === "command" ||
        message.type === "load" ||
        message.type === "state.set" ||
        message.type === "composition.update") &&
      !this.flushViews()
    )
      return Promise.reject(this.terminalError!);
    if (this.rpcCounter >= Number.MAX_SAFE_INTEGER) {
      const error = new FxNodeProtocolError("FxNode RPC identifier counter exhausted");
      this.shutdown(error);
      return Promise.reject(error);
    }
    let id: string;
    do id = `${++this.rpcCounter}:${crypto.randomUUID()}`;
    while (this.pending.has(id));
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { requestType: message.type, owner, resolve: (value) => resolve(value as T), reject });
      try {
        this.worker.postMessage({ protocol: PROTOCOL_VERSION, id, ...message }, transfer);
        onPosted?.();
      } catch (error) {
        this.pending.delete(id);
        const failure = new FxNodeProtocolError("Unable to send a message to the FxNode worker", {
          cause: error,
        });
        this.shutdown(failure);
        reject(failure);
      }
    });
  }
  private flushViews(): boolean {
    for (const view of this.views.values()) if (view.isAttached() && !view.flushPointerLane()) return false;
    return true;
  }
  private expected(version?: number): VersionExpectation {
    return version === undefined ? { kind: "current" } : { kind: "exact", version };
  }
  private compositionExpected(revision?: number): CompositionRevisionExpectation {
    return revision === undefined ? { kind: "current" } : { kind: "exact", revision };
  }
  private compositionChange(update: CompositionUpdateWire): CompositionChange {
    return update.kind === "theme.set" ||
      update.kind === "header-styles.set" ||
      update.kind === "composition.load" ||
      update.kind === "compatibility.set"
      ? { kind: update.kind }
      : { kind: update.kind, id: update.id };
  }
  private sameCompositionChange(left: CompositionChange, right: CompositionChange): boolean {
    return (
      left.kind === right.kind &&
      (left.kind === "theme.set" ||
        left.kind === "header-styles.set" ||
        left.kind === "composition.load" ||
        left.kind === "compatibility.set" ||
        (right.kind !== "theme.set" &&
          right.kind !== "header-styles.set" &&
          right.kind !== "composition.load" &&
          right.kind !== "compatibility.set" &&
          left.id === right.id))
    );
  }
  private protocolFailure(message: string): never {
    const error = new FxNodeProtocolError(message);
    this.shutdown(error);
    throw error;
  }
  private async updateComposition(
    update: CompositionUpdateWire,
    options?: CompositionUpdateOptions,
  ): Promise<CompositionReceipt> {
    const receipt = await this.post<unknown>({
      type: "composition.update",
      expected: this.compositionExpected(options?.expectedRevision),
      update,
    });
    if (!validCompositionReceipt(receipt))
      return this.protocolFailure("FxNode worker returned an invalid composition receipt");
    if (receipt.revision !== this.compositionRevision)
      return this.protocolFailure("FxNode composition receipt does not match the published revision");
    if (options?.expectedRevision !== undefined) {
      const expected = receipt.status === "committed" ? options.expectedRevision + 1 : options.expectedRevision;
      if (receipt.revision !== expected)
        return this.protocolFailure("FxNode composition receipt violates its revision expectation");
    }
    if (receipt.status === "committed") {
      const envelope = this.unclaimedCompositionEvents.get(receipt.revision);
      if (
        !envelope ||
        !this.sameCompositionChange(envelope.change, this.compositionChange(update)) ||
        envelope.graphVersion !== receipt.graphVersion ||
        envelope.graphChanged !== receipt.graphChanged ||
        !envelope.historyReset
      )
        return this.protocolFailure("FxNode composition receipt does not match its composition event");
      this.unclaimedCompositionEvents.delete(receipt.revision);
    }
    return receipt;
  }
  private commandReceipt(value: unknown, expected: VersionExpectation): CommandReceipt {
    if (!validCommandReceipt(value)) return this.protocolFailure("FxNode worker returned an invalid command receipt");
    if (
      expected.kind === "exact" &&
      value.version !== (value.status === "committed" ? expected.version + 1 : expected.version)
    )
      return this.protocolFailure("FxNode worker returned an incoherent command receipt");
    return value;
  }
  private async postCommand(request: RpcRequest, expected: VersionExpectation): Promise<CommandReceipt> {
    return this.commandReceipt(await this.post<unknown>(request), expected);
  }
  dispatch(intent: CommandIntent, options?: { expectedVersion?: number }): Promise<CommandReceipt> {
    let command = intent as Command;
    if (intent.type === "node.add" && !intent.nodeId)
      command = { ...intent, nodeId: crypto.randomUUID() } as unknown as Command;
    if (intent.type === "link.add" && !intent.link.id)
      command = { ...intent, link: { ...intent.link, id: crypto.randomUUID() } } as Command;
    const expected = this.expected(options?.expectedVersion);
    return this.postCommand({ type: "command", command, expected }, expected);
  }
  undo(options?: { expectedVersion?: number }) {
    return this.dispatch({ type: "undo" }, options);
  }
  redo(options?: { expectedVersion?: number }) {
    return this.dispatch({ type: "redo" }, options);
  }
  save() {
    return this.post<GraphLayoutV2>({ type: "save" });
  }
  getSaveData() {
    return this.post<FxNodeSaveData<FxNodeCompositionData>>({ type: "save.data" });
  }
  load(layout: unknown, options?: { expectedVersion?: number }) {
    const expected = this.expected(options?.expectedVersion);
    return this.postCommand({ type: "load", data: layout, expected }, expected);
  }
  getState() {
    return this.post<GraphSnapshot<FxNodeCompositionData>>({ type: "state.get" });
  }
  setState(state: unknown, options?: { expectedVersion?: number }) {
    const expected = this.expected(options?.expectedVersion);
    return this.postCommand({ type: "state.set", state, expected }, expected);
  }
  onMutations(callback: (event: MutationEnvelope<FxNodeCompositionData>) => void) {
    if (this.terminalError) throw this.terminalError;
    if (typeof callback !== "function") throw new TypeError("Mutation callback must be a function");
    this.mutations.add(callback);
    return () => this.mutations.delete(callback);
  }
  onSnapshots(callback: (event: SnapshotEnvelope<FxNodeCompositionData>) => void) {
    if (this.terminalError) throw this.terminalError;
    if (typeof callback !== "function") throw new TypeError("Snapshot callback must be a function");
    this.snapshots.add(callback);
    return () => this.snapshots.delete(callback);
  }
  setTheme(theme: FxNodeTheme, options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "theme.set", theme }, options);
  }
  setHeaderStyles(styles: Readonly<Record<string, FxNodeStyleDefinition>>, options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "header-styles.set", styles }, options);
  }
  setCompatibility(compatibility: FxNodeCompositionData["compatibility"], options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "compatibility.set", compatibility }, options);
  }
  composeSocket(id: string, definition: FxNodeSocketTypeDefinition, options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "socket.compose", id, definition }, options);
  }
  removeSocket(id: string, options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "socket.remove", id }, options);
  }
  composeNode<const D extends FxNodeDefinition>(
    id: string,
    definition: D & NodeReferenceCheck<FxNodeCompositionData, D>,
    options?: CompositionUpdateOptions,
  ) {
    return this.updateComposition({ kind: "node.compose", id, definition }, options);
  }
  removeNode(id: string, options?: CompositionUpdateOptions) {
    return this.updateComposition({ kind: "node.remove", id }, options);
  }
  loadComposition<const T extends FxNodeCompositionData>(
    composition: T & ReferenceCheck<T>,
    options?: CompositionUpdateOptions,
  ) {
    return this.updateComposition({ kind: "composition.load", composition }, options);
  }
  onCompositionChanges(callback: (event: CompositionChangeEnvelope) => void) {
    if (this.terminalError) throw this.terminalError;
    if (typeof callback !== "function") throw new TypeError("Composition callback must be a function");
    this.compositionChanges.add(callback);
    return () => this.compositionChanges.delete(callback);
  }
  async attachView(options: FxNodeViewOptions): Promise<FxNodeView> {
    if (this.terminalError) throw this.terminalError;
    const canvas = options.canvas,
      viewport = decodeFxNodeViewport(options.viewport),
      camera = decodeFxNodeCamera(options.initialCamera ?? { center: { x: 0, y: 0 }, zoom: 1 }),
      area = fxNodeDevicePixels(viewport.width, viewport.height, viewport.dpr);
    if (this.views.size + this.attachingViewCount >= FXNODE_VIEW_LIMITS.maxViews)
      throw new RangeError("FxNode view count limit exceeded");
    const pixels = area ? area.width * area.height : 0;
    if (!area || this.aggregateDevicePixels + pixels > FXNODE_VIEW_LIMITS.maxActiveDevicePixels)
      throw new RangeError("FxNode view exceeds the aggregate pixel budget");
    if (reservedCanvases.has(canvas)) throw new FxNodeCapabilityError("Canvas is already attached", "canvas.in-use");

    reservedCanvases.add(canvas);
    this.attachingViewCount++;
    this.aggregateDevicePixels += pixels;
    let view: FxNodeViewClient | undefined;
    try {
      const context = canvas.getContext("2d");
      if (!context) throw new FxNodeCapabilityError("Canvas 2D context is unavailable", "canvas.2d");
      if (this.terminalError) throw this.terminalError;

      let id: string;
      do id = crypto.randomUUID();
      while (this.usedViewIds.has(id));
      this.usedViewIds.add(id);
      view = new FxNodeViewClient(this, id, canvas, context, viewport, pixels);
      this.views.set(view.id, view);
      this.attachingViewCount--;
      await view.attach(camera);
      return view;
    } catch (error) {
      if (view) view.close(error instanceof Error ? error : new FxNodeProtocolError("Unable to attach view"));
      else {
        if (!this.terminalError) {
          this.attachingViewCount--;
          this.aggregateDevicePixels -= pixels;
        }
        reservedCanvases.delete(canvas);
      }
      throw error;
    }
  }
  releaseView(view: FxNodeViewClient): void {
    if (this.views.get(view.id) === view) {
      this.views.delete(view.id);
      this.aggregateDevicePixels -= view.devicePixels;
    }
  }
  preflightResizeView(view: FxNodeViewClient, pixels: number): void {
    const next = this.aggregateDevicePixels - view.devicePixels + pixels;
    if (next > FXNODE_VIEW_LIMITS.maxActiveDevicePixels)
      throw new RangeError("FxNode view exceeds the aggregate pixel budget");
  }
  commitResizeView(view: FxNodeViewClient, pixels: number): void {
    const next = this.aggregateDevicePixels - view.devicePixels + pixels;
    if (next > FXNODE_VIEW_LIMITS.maxActiveDevicePixels) {
      const error = new FxNodeProtocolError("FxNode worker acknowledged a resize beyond the aggregate pixel budget");
      this.fail(error);
      throw error;
    }
    this.aggregateDevicePixels = next;
    view.devicePixels = pixels;
  }
  rejectViewPending(view: FxNodeViewClient, error: Error): void {
    for (const [id, item] of this.pending)
      if (item.owner === view && item.requestType !== "view.detach") {
        this.pending.delete(id);
        item.reject(error);
      }
  }
  rootError(): Error | undefined {
    return this.terminalError;
  }
  fail(error: Error): void {
    this.shutdown(error);
  }
  get revision(): number {
    return this.compositionRevision ?? 0;
  }
  receipt(value: unknown, expected: VersionExpectation): CommandReceipt {
    return this.commandReceipt(value, expected);
  }
  destroy() {
    this.shutdown(new FxNodeDestroyedError());
  }
  private shutdown(error: Error): void {
    if (this.terminalError) return;
    this.terminalError = error;
    for (const view of [...this.views.values()]) view.close(error);
    this.views.clear();
    this.aggregateDevicePixels = 0;
    this.attachingViewCount = 0;
    for (const item of this.pending.values()) item.reject(error);
    this.pending.clear();
    this.safePost({ protocol: PROTOCOL_VERSION, type: "dispose" });
    this.worker.terminate();
    this.worker.onmessage = null;
    this.worker.onerror = null;
    this.worker.onmessageerror = null;
    this.mutations.clear();
    this.snapshots.clear();
    this.compositionChanges.clear();
    this.unclaimedCompositionEvents.clear();
  }
}

type ViewMessage = Extract<import("./protocol.js").WorkerMessage, { viewId: string }>;
interface ViewControlBatch {
  kind: "viewport" | "render";
  viewport: FxNodeViewport | undefined;
  resolves: Array<() => void>;
  rejects: Array<(error: Error) => void>;
}

class FxNodeViewClient implements FxNodeView {
  private state: "attaching" | "attached" | "detaching" | "detached" | "root-closed" = "attaching";
  devicePixels: number;
  private renderId = 1;
  private surfaceGeneration = 0;
  private deviceWidth: number;
  private deviceHeight: number;
  private hostGeneration = 0;
  private pointerLane = supportsPointerLane() ? createPointerLane() : undefined;
  private latestPointerMove: PointerLaneSnapshot | undefined;
  private lanePointerId: number | undefined;
  private knifePointerId: number | undefined;
  private pendingNodeMenuRequestId: string | undefined;
  private pendingResourceOpenRequestId: string | undefined;
  private selection = emptySelection();
  private hostSnapshot: FxNodeHostSnapshot;
  private readonly hostSubscribers = new Set<() => void>();
  private readonly hostRequests = new Set<(request: FxNodeHostRequest) => void>();
  private readonly barriers = new Map<number, Barrier[]>();
  private detachedError: Error | undefined;
  private detachPromise: Promise<void> | undefined;
  private selectionReady!: () => void;
  private readonly firstSelection = new Promise<void>((resolve) => (this.selectionReady = resolve));
  private activeControl: ViewControlBatch | undefined;
  private readonly queuedControls: ViewControlBatch[] = [];

  constructor(
    private readonly owner: FxNodeClient,
    readonly id: string,
    private readonly canvas: HTMLCanvasElement,
    private readonly context: CanvasRenderingContext2D,
    private viewport: FxNodeViewport,
    devicePixels: number,
  ) {
    this.devicePixels = devicePixels;
    const device = fxNodeDevicePixels(viewport.width, viewport.height, viewport.dpr)!;
    this.deviceWidth = device.width;
    this.deviceHeight = device.height;
    this.hostSnapshot = Object.freeze({
      compositionRevision: owner.revision,
      colorPickerOpen: false,
      selection: this.selection,
    });
  }
  async attach(camera: FxNodeCamera): Promise<void> {
    await this.owner.post(
      {
        type: "view.attach",
        viewId: this.id,
        viewport: this.viewport,
        camera,
        ...(this.pointerLane ? { pointerLane: this.pointerLane } : {}),
      },
      [],
      undefined,
      this,
    );
    await this.firstSelection;
    this.check();
    this.state = "attached";
  }
  consumeMessage(data: ViewMessage): void {
    if (this.detachedError) return;
    if (data.type === "view.frame") this.consumeFrame(data);
    else if (data.type === "view.selection.host") {
      this.selection = Object.freeze({ ...data.projection, mute: Object.freeze({ ...data.projection.mute }) });
      this.selectionReady();
      this.publishHost(this.hostSnapshot.colorPickerOpen);
    } else if (data.type === "view.node-menu.result") {
      if (data.requestId !== this.pendingNodeMenuRequestId) return;
      this.pendingNodeMenuRequestId = undefined;
      if (data.open && data.compositionRevision === this.owner.revision)
        this.notify(
          this.hostRequests,
          Object.freeze({
            kind: "add-node-menu",
            viewPosition: Object.freeze({ ...data.viewPosition }),
            compositionRevision: data.compositionRevision,
          }),
        );
    } else if (data.type === "view.resource.open") {
      if (data.authorization.viewId !== data.viewId) {
        this.owner.fail(new FxNodeProtocolError("FxNode worker returned cross-view resource authorization"));
        return;
      }
      if (data.requestId !== this.pendingResourceOpenRequestId) return;
      this.pendingResourceOpenRequestId = undefined;
      if (data.authorization.compositionRevision === this.owner.revision)
        this.notify(
          this.hostRequests,
          Object.freeze({
            kind: "resource-open",
            authorization: Object.freeze({ ...data.authorization }),
            resource: Object.freeze({ ...data.resource, accept: Object.freeze([...data.resource.accept]) }),
          }),
        );
    }
  }
  private consumeFrame(data: Extract<ViewMessage, { type: "view.frame" }>): void {
    if (data.hostGeneration > this.hostGeneration) {
      this.owner.fail(new FxNodeProtocolError("FxNode worker used a future host generation"));
      return;
    }
    if (data.surfaceGeneration < this.surfaceGeneration) {
      this.owner.requiredPost({
        protocol: PROTOCOL_VERSION,
        type: "view.frame.consumed",
        viewId: this.id,
        frameId: data.frameId,
      });
      return;
    }
    if (
      data.surfaceGeneration > this.surfaceGeneration ||
      data.deviceWidth !== this.deviceWidth ||
      data.deviceHeight !== this.deviceHeight
    ) {
      this.owner.fail(new FxNodeProtocolError("FxNode worker returned an incoherent view surface"));
      return;
    }
    let error: unknown;
    try {
      this.context.drawImage(data.bitmap, 0, 0, this.canvas.width, this.canvas.height);
    } catch (cause) {
      error = cause;
    }
    if (data.hostGeneration === this.hostGeneration) this.publishHost(data.host.colorPickerOpen);
    this.owner.requiredPost({
      protocol: PROTOCOL_VERSION,
      type: "view.frame.consumed",
      viewId: this.id,
      frameId: data.frameId,
    });
    for (const [id, list] of [...this.barriers])
      if (id <= data.renderId) {
        for (const barrier of list) error ? barrier.reject(error) : barrier.resolve();
        this.barriers.delete(id);
      }
  }
  private notify<T>(callbacks: ReadonlySet<(event: T) => void>, event: T): void {
    for (const callback of callbacks)
      try {
        callback(event);
      } catch (error) {
        console.error("FxNode subscriber failed", error);
      }
  }
  private publishHost(colorPickerOpen: boolean): void {
    this.hostSnapshot = Object.freeze({
      compositionRevision: this.owner.revision,
      colorPickerOpen,
      selection: this.selection,
    });
    this.notify(this.hostSubscribers, undefined);
  }
  clearDocumentDependentHostState(revision = this.owner.revision): void {
    if (this.state !== "attaching" && this.state !== "attached") return;
    this.invalidateHostInteractions();
    this.pendingNodeMenuRequestId = this.pendingResourceOpenRequestId = undefined;
    this.hostSnapshot = Object.freeze({
      compositionRevision: revision,
      colorPickerOpen: false,
      selection: this.selection,
    });
    this.notify(this.hostSubscribers, undefined);
  }
  private invalidateHostInteractions(): number {
    if (this.hostGeneration >= Number.MAX_SAFE_INTEGER) {
      const error = new FxNodeProtocolError("FxNode host generation counter exhausted");
      this.owner.fail(error);
      throw error;
    }
    return ++this.hostGeneration;
  }
  private nextRenderId(): number {
    if (this.renderId >= Number.MAX_SAFE_INTEGER) {
      const error = new FxNodeProtocolError("FxNode render counter exhausted");
      this.owner.fail(error);
      throw error;
    }
    return ++this.renderId;
  }
  private followingRenderId(): number {
    if (this.renderId >= Number.MAX_SAFE_INTEGER) {
      const error = new FxNodeProtocolError("FxNode render counter exhausted");
      this.owner.fail(error);
      throw error;
    }
    return this.renderId + 1;
  }
  private followingHostGeneration(): number {
    if (this.hostGeneration >= Number.MAX_SAFE_INTEGER) {
      const error = new FxNodeProtocolError("FxNode host generation counter exhausted");
      this.owner.fail(error);
      throw error;
    }
    return this.hostGeneration + 1;
  }
  private check(): void {
    const rootError = this.owner.rootError();
    if (rootError) throw rootError;
    if (this.detachedError) throw this.detachedError;
  }
  isAttached(): boolean {
    return this.state === "attached";
  }
  private nextPointerFence(): PointerFence | undefined {
    if (!this.pointerLane) return;
    const generation = (pointerLaneFence(this.pointerLane) + 1) | 0;
    return this.latestPointerMove ? { generation, before: this.latestPointerMove } : { generation };
  }
  flushPointerLane(): boolean {
    if (!this.isAttached()) return true;
    const pointerFence = this.nextPointerFence();
    if (!pointerFence) return true;
    const sent = this.owner.requiredPost({
      protocol: PROTOCOL_VERSION,
      type: "view.pointer.flush",
      viewId: this.id,
      pointerFence,
    });
    if (sent) advancePointerLaneFence(this.pointerLane!);
    return sent;
  }
  private sendInput(event: InputEventWire, nodeMenuRequestId?: string, resourceOpenRequestId?: string): void {
    const pointerFence = this.nextPointerFence();
    if (
      !this.owner.requiredPost({
        protocol: PROTOCOL_VERSION,
        type: "view.input",
        viewId: this.id,
        event,
        hostGeneration: this.hostGeneration,
        ...(pointerFence ? { pointerFence } : {}),
        ...(nodeMenuRequestId ? { nodeMenuRequestId } : {}),
        ...(resourceOpenRequestId ? { resourceOpenRequestId } : {}),
      })
    )
      throw this.owner.rootError()!;
    if (pointerFence) advancePointerLaneFence(this.pointerLane!);
  }
  readonly feedInput = (input: FxNodeInput): void => {
    this.check();
    const wire = decodeFxNodeInput(input);
    if (wire.kind !== "pointer" || wire.phase !== "move" || wire.buttons !== 0) this.invalidateHostInteractions();
    if (
      wire.kind === "pointer" &&
      wire.phase === "move" &&
      this.pointerLane &&
      this.knifePointerId !== wire.pointerId &&
      (this.lanePointerId === undefined || this.lanePointerId === wire.pointerId)
    ) {
      const sequence = publishPointerMove(this.pointerLane, wire as PointerMoveWire, this.hostGeneration);
      if (sequence !== undefined) {
        this.latestPointerMove = { sequence, hostGeneration: this.hostGeneration, event: wire as PointerMoveWire };
        return;
      }
    }
    const menu =
      wire.kind === "pointer" &&
      wire.phase === "down" &&
      wire.button === 2 &&
      (wire.modifiers & 2) === 0 &&
      (wire.buttons & 1) === 0
        ? crypto.randomUUID()
        : undefined;
    const resource =
      wire.kind === "pointer" && wire.phase === "down" && wire.button === 0 ? crypto.randomUUID() : undefined;
    this.sendInput(wire, menu, resource);
    if ((wire.kind === "pointer" && wire.phase === "down") || wire.kind === "wheel" || wire.kind === "key") {
      this.pendingNodeMenuRequestId = undefined;
      this.pendingResourceOpenRequestId = undefined;
    }
    if (menu) this.pendingNodeMenuRequestId = menu;
    if (resource) this.pendingResourceOpenRequestId = resource;
    if (wire.kind === "pointer" && wire.phase === "down") {
      this.lanePointerId ??= wire.pointerId;
      if (wire.button === 2 && (wire.modifiers & 2) !== 0) this.knifePointerId = wire.pointerId;
    }
    if (wire.kind === "pointer" && (wire.phase === "up" || wire.phase === "cancel")) {
      if (this.lanePointerId === wire.pointerId) this.lanePointerId = undefined;
      if (this.knifePointerId === wire.pointerId) this.knifePointerId = undefined;
    }
  };
  private enqueueControl(kind: ViewControlBatch["kind"], viewport?: FxNodeViewport): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const last = this.queuedControls.at(-1);
      if (kind === "viewport" && last?.kind === "viewport") {
        last.viewport = viewport;
        last.resolves.push(resolve);
        last.rejects.push(reject);
      } else this.queuedControls.push({ kind, viewport, resolves: [resolve], rejects: [reject] });
      this.pumpControls();
    });
  }
  private pumpControls(): void {
    if (this.activeControl || this.detachedError) return;
    const batch = this.queuedControls.shift();
    if (!batch) return;
    this.activeControl = batch;
    void this.executeControl(batch)
      .then(() => batch.resolves.forEach((resolve) => resolve()))
      .catch((cause) => {
        const error = cause instanceof Error ? cause : new FxNodeProtocolError("FxNode view control failed");
        batch.rejects.forEach((reject) => reject(error));
      })
      .finally(() => {
        this.activeControl = undefined;
        this.pumpControls();
      });
  }
  private async executeControl(batch: ViewControlBatch): Promise<void> {
    this.check();
    if (batch.kind === "render") return this.postRenderBarrier();
    const viewport = batch.viewport!,
      area = fxNodeDevicePixels(viewport.width, viewport.height, viewport.dpr)!;
    if (
      viewport.width === this.viewport.width &&
      viewport.height === this.viewport.height &&
      viewport.dpr === this.viewport.dpr
    )
      return;
    const pixels = area.width * area.height;
    this.owner.preflightResizeView(this, pixels);
    const expectedSurfaceGeneration = this.surfaceGeneration + 1;
    if (!Number.isSafeInteger(expectedSurfaceGeneration)) {
      const error = new FxNodeProtocolError("FxNode surface generation counter exhausted");
      this.owner.fail(error);
      throw error;
    }
    const pointerFence = this.nextPointerFence(),
      oldNodeMenuRequestId = this.pendingNodeMenuRequestId,
      oldResourceOpenRequestId = this.pendingResourceOpenRequestId,
      hostGeneration = this.followingHostGeneration();
    await this.owner.post<void>(
      {
        type: "view.viewport",
        viewId: this.id,
        viewport,
        expectedSurfaceGeneration,
        hostGeneration,
        ...(pointerFence ? { pointerFence } : {}),
      },
      [],
      () => {
        this.hostGeneration = hostGeneration;
        if (pointerFence) advancePointerLaneFence(this.pointerLane!);
        if (this.pendingNodeMenuRequestId === oldNodeMenuRequestId) this.pendingNodeMenuRequestId = undefined;
        if (this.pendingResourceOpenRequestId === oldResourceOpenRequestId)
          this.pendingResourceOpenRequestId = undefined;
      },
      this,
    );
    this.check();
    this.viewport = viewport;
    this.deviceWidth = area.width;
    this.deviceHeight = area.height;
    this.surfaceGeneration = expectedSurfaceGeneration;
    this.owner.commitResizeView(this, pixels);
  }
  readonly setViewport = async (value: FxNodeViewport): Promise<void> => {
    this.check();
    const viewport = decodeFxNodeViewport(value),
      area = fxNodeDevicePixels(viewport.width, viewport.height, viewport.dpr);
    if (!area) throw new RangeError("FxNode view exceeds the aggregate pixel budget");
    await this.enqueueControl("viewport", viewport);
  };
  readonly getHostSnapshot = (): FxNodeHostSnapshot => {
    this.check();
    return this.hostSnapshot;
  };
  readonly subscribeHost = (callback: () => void): (() => void) => {
    this.check();
    if (typeof callback !== "function") throw new TypeError("Host subscriber must be a function");
    this.hostSubscribers.add(callback);
    return () => this.hostSubscribers.delete(callback);
  };
  readonly onHostRequests = (callback: (request: FxNodeHostRequest) => void): (() => void) => {
    this.check();
    if (typeof callback !== "function") throw new TypeError("Host request callback must be a function");
    this.hostRequests.add(callback);
    return () => this.hostRequests.delete(callback);
  };
  private action(
    request: RpcRequest,
    expected: VersionExpectation,
    transfer: Transferable[] = [],
  ): Promise<CommandReceipt> {
    const fenced = this.nextPointerFence();
    return this.owner
      .post<unknown>(
        { ...request, ...(fenced ? { pointerFence: fenced } : {}) } as RpcRequest,
        transfer,
        fenced ? () => advancePointerLaneFence(this.pointerLane!) : undefined,
        this,
      )
      .then((value) => this.owner.receipt(value, expected));
  }
  async addNode(params: AddNodeParams, options?: FxNodeActionOptions): Promise<CommandReceipt> {
    this.check();
    const p = decodeFxNodeAddNodeParams(params),
      expected = decodeFxNodeActionOptions(options);
    return this.action(
      {
        type: "view.node.add",
        viewId: this.id,
        nodeId: p.nodeId ?? crypto.randomUUID(),
        typeId: p.typeId,
        viewPosition: p.viewPosition,
        expected,
      },
      expected,
    );
  }
  async removeSelected(options?: FxNodeActionOptions): Promise<CommandReceipt> {
    this.check();
    const expected = decodeFxNodeActionOptions(options);
    return this.action({ type: "view.selection.remove", viewId: this.id, expected }, expected);
  }
  async setSelectedMuted(value: boolean, options?: FxNodeActionOptions): Promise<CommandReceipt> {
    this.check();
    if (typeof value !== "boolean") throw new TypeError("Muted state must be boolean");
    const expected = decodeFxNodeActionOptions(options);
    return this.action({ type: "view.selection.mute", viewId: this.id, value, expected }, expected);
  }
  async provideResource(
    authorization: FxNodeResourceAuthorization,
    data: FxNodeResourceData,
    options?: FxNodeActionOptions,
  ): Promise<CommandReceipt> {
    this.check();
    const auth = decodeFxNodeResourceAuthorization(authorization);
    if (auth.viewId !== this.id)
      throw new FxNodeCapabilityError("Resource authorization belongs to another view", "resource.view-mismatch");
    const resource = decodeFxNodeResourceData(data),
      expected = decodeFxNodeActionOptions(options);
    return this.action(
      { type: "view.resource.set", viewId: this.id, authorization: auth, resource, expected },
      expected,
      [resource.bytes],
    );
  }
  async whenRendered(): Promise<void> {
    this.check();
    return this.enqueueControl("render");
  }
  private async postRenderBarrier(): Promise<void> {
    const id = this.nextRenderId(),
      pointerFence = this.nextPointerFence(),
      barrier = new Promise<void>((resolve, reject) =>
        this.barriers.set(id, [...(this.barriers.get(id) ?? []), { resolve, reject }]),
      );
    if (
      !this.owner.requiredPost({
        protocol: PROTOCOL_VERSION,
        type: "view.render",
        viewId: this.id,
        renderId: id,
        hostGeneration: this.hostGeneration,
        ...(pointerFence ? { pointerFence } : {}),
      })
    )
      throw this.owner.rootError()!;
    if (pointerFence) advancePointerLaneFence(this.pointerLane!);
    await barrier;
  }
  detach(): Promise<void> {
    if (!this.detachPromise) {
      const rootError = this.owner.rootError();
      if (rootError) return (this.detachPromise = Promise.reject(rootError));
      this.state = "detaching";
      const detached = new FxNodeViewDetachedError();
      this.detachedError = detached;
      this.hostSubscribers.clear();
      this.hostRequests.clear();
      this.pendingNodeMenuRequestId = this.pendingResourceOpenRequestId = undefined;
      this.owner.rejectViewPending(this, detached);
      const controls = [...(this.activeControl ? [this.activeControl] : []), ...this.queuedControls];
      this.queuedControls.length = 0;
      for (const batch of controls) batch.rejects.forEach((reject) => reject(detached));
      for (const list of this.barriers.values()) for (const barrier of list) barrier.reject(detached);
      this.barriers.clear();
      this.detachPromise = this.owner
        .post<void>({ type: "view.detach", viewId: this.id }, [], undefined, this)
        .then(() => {
          this.state = "detached";
          this.release();
        })
        .catch((cause) => {
          const rootError = this.owner.rootError();
          if (rootError) throw rootError;
          const error = new FxNodeProtocolError("FxNode worker failed to detach a view", { cause });
          this.owner.fail(error);
          throw error;
        });
    }
    return this.detachPromise;
  }
  private release(): void {
    reservedCanvases.delete(this.canvas);
    this.owner.releaseView(this);
    this.hostSubscribers.clear();
    this.hostRequests.clear();
    this.selectionReady();
  }
  close(error: Error): void {
    if (this.state === "root-closed" || this.state === "detached") return;
    this.state = "root-closed";
    this.detachedError = error;
    this.release();
    const controls = [...(this.activeControl ? [this.activeControl] : []), ...this.queuedControls];
    this.queuedControls.length = 0;
    for (const batch of controls) batch.rejects.forEach((reject) => reject(error));
    for (const list of this.barriers.values()) for (const barrier of list) barrier.reject(error);
    this.barriers.clear();
  }
}

export async function createFxNode(options: CreateFxNodeOptions): Promise<FxNode> {
  const { applicationId, applicationVersion, resources, historyLimit = 100, workerUrl } = options;
  if (!Number.isSafeInteger(historyLimit) || historyLimit < 0 || historyLimit > 1000)
    throw new RangeError("historyLimit must be an integer from 0 to 1000");
  if (typeof Worker === "undefined")
    throw new FxNodeCapabilityError("FxNode requires module Worker support", "worker.missing");
  if (typeof crypto?.randomUUID !== "function")
    throw new FxNodeCapabilityError("FxNode requires crypto.randomUUID", "crypto.random-uuid.missing");
  const bootstrap = createInitialFxNodeComposition(applicationId, applicationVersion, resources).source;
  const url =
    workerUrl === undefined
      ? new URL(defaultWorkerUrl, import.meta.url)
      : workerUrl instanceof URL
        ? workerUrl
        : new URL(workerUrl, import.meta.url);
  let worker: Worker;
  try {
    worker = new Worker(url, { type: "module" });
  } catch {
    throw new FxNodeCapabilityError(
      "Unable to construct the FxNode module worker. Check workerUrl and CSP worker-src.",
      "worker.construct",
    );
  }
  const client = new FxNodeClient(worker);
  try {
    await client.initialize(bootstrap.id, bootstrap.version, bootstrap.resources, historyLimit);
    return client;
  } catch (error) {
    client.destroy();
    throw error;
  }
}
