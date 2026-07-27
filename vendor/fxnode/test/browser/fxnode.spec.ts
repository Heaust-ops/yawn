import { test, expect } from "@playwright/test";
test("host snapshots are stable immutable projections with isolated subscriptions", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const api = window.root,
      view = window.view,
      first = view.getHostSnapshot(),
      same = first === view.getHostSnapshot(),
      frozen = Object.isFrozen(first) && Object.isFrozen(first.selection) && Object.isFrozen(first.selection.mute);
    let good = 0,
      bad = 0;
    const offBad = view.subscribeHost(() => {
        bad++;
        throw new Error("subscriber");
      }),
      offGood = view.subscribeHost(() => good++);
    await api.composeNode("phase-one", {
      version: 1,
      title: "Phase One",
      behavior: "standard",
      style: "shader",
      parameters: {},
      sockets: {},
      ui: [],
      muteBypass: [],
      migrations: [],
    });
    const projected = view.getHostSnapshot();
    offBad();
    offBad();
    offGood();
    await api.removeNode("phase-one");
    const detached = structuredClone(projected);
    api.destroy();
    let readable = true,
      terminal = "";
    try {
      view.getHostSnapshot();
    } catch (e) {
      readable = false;
      terminal = (e as Error).name;
    }
    return {
      same,
      frozen,
      good,
      bad,
      firstRevision: first.compositionRevision,
      revision: projected.compositionRevision,
      detachedKeys: Object.keys(detached).sort(),
      readable,
      terminal,
    };
  });
  expect(result).toEqual({
    same: true,
    frozen: true,
    good: 1,
    bad: 1,
    firstRevision: 31,
    revision: 32,
    detachedKeys: ["colorPickerOpen", "compositionRevision", "selection"],
    readable: false,
    terminal: "FxNodeDestroyedError",
  });
});
test("getState is detached and setState is atomic, ordered, and undoable", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const result = await page.evaluate(async () => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      original = await api.getState(),
      detached = structuredClone(original);
    (detached.nodes as unknown[]).push({});
    const events: string[] = [];
    api.onMutations((event) => events.push(`mutation:${event.version}`));
    api.onSnapshots((event) => events.push(`snapshot:${event.version}`));
    const noop = await api.setState(original),
      target = { ...original, graphId: "replacement" as typeof original.graphId },
      changed = await api.setState(target, { expectedVersion: original.version }),
      after = await api.getState(),
      undo = await api.undo(),
      undone = await api.getState();
    api.destroy();
    return {
      originalVersion: original.version,
      originalNodes: original.nodes.length,
      detachedNodes: detached.nodes.length,
      noop,
      changed,
      afterGraphId: after.graphId,
      undo,
      undoneGraphId: undone.graphId,
      events,
    };
  });
  const v = result.originalVersion;
  expect(result.detachedNodes).toBe(result.originalNodes + 1);
  expect(result.noop).toEqual({ status: "noop", version: v });
  expect(result.changed).toEqual({ status: "committed", version: v + 1 });
  expect(result.afterGraphId).toBe("replacement");
  expect(result.undo).toEqual({ status: "committed", version: v + 2 });
  expect(result.undoneGraphId).not.toBe("replacement");
  expect(result.events).toEqual([`mutation:${v + 1}`, `snapshot:${v + 1}`, `mutation:${v + 2}`, `snapshot:${v + 2}`]);
});
test("explicit node and selection actions preserve worker authority and structured failures", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const api = window.root,
      view = window.view,
      initial = await api.getState();
    let notifications = 0,
      selection = JSON.stringify(view.getHostSnapshot().selection);
    const off = view.subscribeHost(() => {
      const next = JSON.stringify(view.getHostSnapshot().selection);
      if (next !== selection) {
        selection = next;
        notifications++;
      }
    });
    let stale = "",
      unknown = "",
      duplicate = "";
    try {
      await view.removeSelected({ expectedVersion: initial.version + 1 });
    } catch (error) {
      stale = (error as Error & { code?: string }).code ?? "";
    }
    try {
      await view.addNode({ typeId: "missing", viewPosition: { x: 40, y: 40 }, nodeId: "unknown" });
    } catch (error) {
      unknown = (error as Error & { code?: string }).code ?? "";
    }
    const added = await view.addNode({
        typeId: "fxnode.shader.value",
        viewPosition: { x: 40, y: 40 },
        nodeId: "supplied-id",
      }),
      selected = structuredClone(view.getHostSnapshot().selection);
    try {
      await view.addNode({ typeId: "fxnode.shader.value", viewPosition: { x: 80, y: 80 }, nodeId: "supplied-id" });
    } catch (error) {
      duplicate = (error as Error & { code?: string }).code ?? "";
    }
    const muted = await view.setSelectedMuted(true),
      mutedSelection = structuredClone(view.getHostSnapshot().selection),
      afterMute = await api.getState();
    const removed = await view.removeSelected(),
      afterRemove = await api.getState(),
      empty = structuredClone(view.getHostSnapshot().selection);
    off();
    return {
      stale,
      unknown,
      duplicate,
      added,
      selected,
      muted,
      mutedSelection,
      node: afterMute.nodes.find((node) => node.id === "supplied-id"),
      removed,
      remaining: afterRemove.nodes.length,
      empty,
      notifications,
    };
  });
  expect(result.stale).toBe("version.stale");
  expect(result.unknown).toBe("node.type-unknown");
  expect(result.duplicate).toBe("node.duplicate");
  expect(result.added).toEqual({ status: "committed", version: 2 });
  expect(result.selected).toEqual({
    nodeCount: 1,
    linkCount: 0,
    canRemove: true,
    mute: { enabled: true, state: "all-unmuted" },
  });
  expect(result.muted).toEqual({ status: "committed", version: 3 });
  expect(result.node?.muted).toBe(true);
  expect(result.mutedSelection.mute).toEqual({ enabled: true, state: "all-muted" });
  expect(result.removed).toEqual({ status: "committed", version: 4 });
  expect(result.remaining).toBe(0);
  expect(result.empty).toEqual({ nodeCount: 0, linkCount: 0, canRemove: false, mute: { enabled: false } });
  expect(result.notifications).toBe(3);
});
test("dedicated worker renders, commits FIFO, fans out and destroys", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(HTMLCanvasElement.prototype, "transferControlToOffscreen", {
      value: () => {
        throw new Error("sabotaged");
      },
    });
    Object.defineProperty(window, "OffscreenCanvas", { value: undefined });
  });
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const api = window.root;
    const trace: Array<
      { kind: "mutation"; baseVersion: number; version: number; cause: string } | { kind: "snapshot"; version: number }
    > = [];
    api.onMutations((event) =>
      trace.push({ kind: "mutation", baseVersion: event.baseVersion, version: event.version, cause: event.cause }),
    );
    api.onSnapshots((event) => trace.push({ kind: "snapshot", version: event.version }));

    const [a, b] = await Promise.all([
      api.dispatch({ type: "node.add", nodeType: "fxnode.shader.value", position: { x: 0, y: 0 } }),
      api.dispatch({ type: "node.add", nodeType: "fxnode.shader.value", position: { x: 1, y: 1 } }),
    ]);
    const beforeStale = { trace: structuredClone(trace), snapshot: await api.getState() };
    let staleError: { name: string; message: string; code?: string } | undefined;
    try {
      await api.dispatch({ type: "undo" }, { expectedVersion: 0 });
    } catch (error) {
      const value = error as Error & { code?: string };
      staleError = { name: value.name, message: value.message, ...(value.code ? { code: value.code } : {}) };
    }
    const afterStale = { trace: structuredClone(trace), snapshot: await api.getState() };
    const undo = await api.undo();
    const redo = await api.redo();

    const beforeQueries = trace.length;
    const snap = await api.getState();
    const saved = await api.save();
    const queriesSilent = trace.length === beforeQueries;
    const mirror = document.querySelector<HTMLCanvasElement>("#mirror");
    const copy = document.querySelector<HTMLCanvasElement>("#copy");
    const primary = document.querySelector<HTMLCanvasElement>("#primary");
    if (!mirror || !copy || !primary) throw new Error("Canvas missing");
    mirror.getContext("2d")!.drawImage(primary, 0, 0);
    copy.getContext("2d")!.drawImage(primary, 0, 0);
    const pixel = (canvas: HTMLCanvasElement) => Array.from(canvas.getContext("2d")!.getImageData(2, 2, 1, 1).data);
    const pixels = [pixel(primary), pixel(mirror), pixel(copy)];
    api.destroy();
    let destroyed = false;
    try {
      await api.getState();
    } catch {
      destroyed = true;
    }
    return {
      a,
      b,
      undo,
      redo,
      trace,
      beforeStale,
      afterStale,
      staleError,
      queriesSilent,
      pixels,
      catalog: saved.catalogVersion,
      nodes: snap.nodes.length,
      destroyed,
    };
  });

  expect(result.a).toEqual({ status: "committed", version: 2 });
  expect(result.b).toEqual({ status: "committed", version: 3 });
  expect(result.beforeStale.trace).toEqual([
    { kind: "mutation", baseVersion: 1, version: 2, cause: "api" },
    { kind: "snapshot", version: 2 },
    { kind: "mutation", baseVersion: 2, version: 3, cause: "api" },
    { kind: "snapshot", version: 3 },
  ]);
  expect(result.beforeStale.snapshot.version).toBe(3);
  expect(result.staleError).toEqual({
    name: "FxNodeWorkerError",
    message: "Expected version does not match",
    code: "version.stale",
  });
  expect(result.afterStale).toEqual(result.beforeStale);
  expect(result.undo).toEqual({ status: "committed", version: 4 });
  expect(result.redo).toEqual({ status: "committed", version: 5 });
  expect(result.trace).toEqual([
    ...result.beforeStale.trace,
    { kind: "mutation", baseVersion: 3, version: 4, cause: "undo" },
    { kind: "snapshot", version: 4 },
    { kind: "mutation", baseVersion: 4, version: 5, cause: "redo" },
    { kind: "snapshot", version: 5 },
  ]);
  expect(result.queriesSilent).toBe(true);
  expect(result.catalog).toBe(4);
  expect(result.nodes).toBe(2);
  expect(result.destroyed).toBe(true);
  expect(result.pixels[0]).toEqual(result.pixels[1]);
  expect(result.pixels[0]).toEqual(result.pixels[2]);
  expect(result.pixels[0]?.[3]).toBe(255);
});

test("steady-size frame presentation does not reallocate canvas backing stores", async ({ page }) => {
  await page.addInitScript(() => {
    const counts = { width: 0, height: 0 };
    (window as unknown as { canvasAssignments: typeof counts }).canvasAssignments = counts;
    for (const key of ["width", "height"] as const) {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, key)!;
      Object.defineProperty(HTMLCanvasElement.prototype, key, {
        ...descriptor,
        set(value: number) {
          counts[key]++;
          descriptor.set!.call(this, value);
        },
      });
    }
  });
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  await page.evaluate(async () => {
    const counts = (window as unknown as { canvasAssignments: { width: number; height: number } }).canvasAssignments;
    counts.width = counts.height = 0;
    for (let index = 0; index < 8; index++)
      await window.root.dispatch({
        type: "node.add",
        nodeType: "fxnode.shader.value",
        position: { x: index * 10, y: index * 10 },
      });
    await window.view.whenRendered();
  });
  expect(
    await page.evaluate(
      () => (window as unknown as { canvasAssignments: { width: number; height: number } }).canvasAssignments,
    ),
  ).toEqual({ width: 0, height: 0 });
});

test("structured command errors with paths reject only their RPC", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    let error: { name?: string; code?: string; path?: string } = {};
    try {
      await window.root.dispatch({
        type: "link.add",
        link: {
          id: "stale",
          fromNodeId: "missing-a",
          fromSocketId: "missing-a:out",
          toNodeId: "missing-b",
          toSocketId: "missing-b:in",
          muted: false,
          extensions: {},
        },
      } as any);
    } catch (value) {
      const e = value as typeof error;
      error = {
        ...(e.name === undefined ? {} : { name: e.name }),
        ...(e.code === undefined ? {} : { code: e.code }),
        ...(e.path === undefined ? {} : { path: e.path }),
      };
    }
    const snapshot = await window.root.getState();
    window.root.destroy();
    return { error, version: snapshot.version };
  });
  expect(result).toEqual({
    error: { name: "FxNodeWorkerError", code: "link.endpoint", path: "/links/stale" },
    version: 1,
  });
});

test("malformed and incoherent state receipts terminate the client", async ({ browser }) => {
  const installCorruptor = () => {
    const NativeWorker = Worker;
    let corruptType = "";
    class CorruptingWorker extends NativeWorker {
      private requestId = "";
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        super.addEventListener("message", (event) => {
          const message = event.data;
          if (!this.requestId || message?.type !== "response" || message.id !== this.requestId) return;
          event.stopImmediatePropagation();
          const type = corruptType,
            id = this.requestId;
          this.requestId = "";
          queueMicrotask(() =>
            this.dispatchEvent(
              new MessageEvent("message", {
                data: {
                  protocol: 2,
                  type: "response",
                  id,
                  ok: true,
                  value: type === "state.set" ? null : { status: "noop", version: 2 },
                },
              }),
            ),
          );
        });
      }
      override postMessage(message: unknown, options?: StructuredSerializeOptions | Transferable[]) {
        if ((message as { type?: string }).type === corruptType) this.requestId = (message as { id: string }).id;
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperty(window, "corruptReceiptType", { set: (value: string) => (corruptType = value) });
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: CorruptingWorker });
  };
  const statePage = await browser.newPage();
  await statePage.addInitScript(installCorruptor);
  await statePage.goto("/test/browser/index.html");
  await statePage.evaluate(() => window.ready);
  const malformed = await statePage.evaluate(async () => {
    const state = await window.root.getState();
    (window as unknown as { corruptReceiptType: string }).corruptReceiptType = "state.set";
    let first = "",
      terminal = "";
    try {
      await window.root.setState(state, { expectedVersion: state.version });
    } catch (error) {
      first = (error as Error).name;
    }
    try {
      await window.root.getState();
    } catch (error) {
      terminal = (error as Error).name;
    }
    return { first, terminal };
  });
  await statePage.close();
  const loadPage = await browser.newPage();
  await loadPage.addInitScript(installCorruptor);
  await loadPage.goto("/test/browser/index.html");
  await loadPage.evaluate(() => window.ready);
  const incoherent = await loadPage.evaluate(async () => {
    const state = await window.root.getState(),
      data = await window.root.getSaveData();
    (window as unknown as { corruptReceiptType: string }).corruptReceiptType = "load";
    let first = "",
      terminal = "";
    try {
      await window.root.load(data, { expectedVersion: state.version });
    } catch (error) {
      first = (error as Error).name;
    }
    try {
      await window.root.getState();
    } catch (error) {
      terminal = (error as Error).name;
    }
    return { first, terminal };
  });
  await loadPage.close();
  expect(malformed).toEqual({ first: "FxNodeProtocolError", terminal: "FxNodeProtocolError" });
  expect(incoherent).toEqual({ first: "FxNodeProtocolError", terminal: "FxNodeProtocolError" });
});
