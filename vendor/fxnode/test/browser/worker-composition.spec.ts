import { expect, test } from "@playwright/test";

test("real worker migrates historical custom graph atomically and keeps init state private", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      original = Worker.prototype.postMessage;
    let initHasValue = false;
    Worker.prototype.postMessage = function (this: Worker, message: any, transfer?: Transferable[]) {
      if (message?.type === "init") {
        const id = message.id;
        this.addEventListener("message", (e: any) => {
          if (e.data?.type === "response" && e.data.id === id) initHasValue = "value" in e.data;
        });
      }
      return Reflect.apply(original, this, [message, transfer ?? []]);
    } as typeof Worker.prototype.postMessage;
    const api = await h.createMigration();
    Worker.prototype.postMessage = original;
    const initial = await api.getState(),
      saved = await api.save();
    const edit = await api.dispatch({
      type: "node.parameter",
      id: "left",
      key: "level",
      value: { kind: "number", value: 12 },
    });
    const edited = await api.getState();
    let mutations = 0,
      snapshots = 0;
    api.onMutations(() => mutations++);
    api.onSnapshots(() => snapshots++);
    const malformed = structuredClone(saved);
    delete malformed.nodes[0].parameters.level;
    let code = "";
    try {
      await api.load(malformed);
    } catch (e) {
      code = (e as any).code ?? "";
    }
    await new Promise((resolve) => setTimeout(resolve, 30));
    const after = await api.getState();
    api.destroy();
    return { initHasValue, initial, saved, edit, edited, code, mutations, snapshots, after };
  });
  expect(result.initHasValue).toBe(false);
  expect(result.initial.catalogVersion).toBe(73);
  expect(result.saved.catalogVersion).toBe(73);
  expect(result.saved.nodes.every((n: any) => n.typeVersion === 2 && n.parameters.level)).toBe(true);
  expect(result.saved.links[0]).toMatchObject({
    id: "historical-link",
    fromSocketId: "left:source",
    toSocketId: "right:sink",
    extensions: { kept: true },
  });
  expect(result.edit).toEqual({ status: "committed", version: 2 });
  expect(result.edited.nodes.find((n: any) => n.id === "left").parameters.level.value).toBe(12);
  expect(result.code).toBe("catalog.invalid");
  expect(result.mutations).toBe(0);
  expect(result.snapshots).toBe(0);
  expect(result.after).toEqual(result.edited);
});

test("command-log API is worker-authoritative, side-effect-free to query, and atomically replayable", async ({
  page,
}) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const api = await (window as any).workerCompositionTest.createA(),
      mutations: any[] = [],
      snapshots: any[] = [];
    api.onMutations((event: any) => mutations.push(event));
    api.onSnapshots((event: any) => snapshots.push(event));
    const initial = await api.getState(),
      empty = await api.getSaveData(),
      afterQuery = await api.getState(),
      queryEvents = { mutations: mutations.length, snapshots: snapshots.length };
    await api.dispatch({ type: "node.add", nodeId: "logged", nodeType: "alpha-node", position: { x: 12, y: 34 } });
    const populated = await api.getSaveData(),
      eventsAfterAdd = { mutations: mutations.length, snapshots: snapshots.length };
    const noop = await api.load(populated),
      afterNoop = await api.getState(),
      eventsAfterNoop = { mutations: mutations.length, snapshots: snapshots.length };
    const undo = await api.undo(),
      afterUndo = await api.getState(),
      undoneSave = await api.getSaveData();
    const changed = await api.load(populated),
      afterChanged = await api.getState(),
      eventsAfterChanged = { mutations: mutations.length, snapshots: snapshots.length };
    const malformed = structuredClone(populated);
    malformed.commands.push({ type: "node.remove", id: "missing" });
    let malformedCode = "";
    try {
      await api.load(malformed);
    } catch (error) {
      malformedCode = (error as any).code ?? "";
    }
    const afterMalformed = await api.getState(),
      afterMalformedSave = await api.getSaveData(),
      eventsAfterMalformed = { mutations: mutations.length, snapshots: snapshots.length };
    const legacy = await api.save(),
      legacyResult = await api.load(legacy),
      legacySaveData = await api.getSaveData();
    api.destroy();
    return {
      initial,
      empty,
      afterQuery,
      queryEvents,
      populated,
      eventsAfterAdd,
      noop,
      afterNoop,
      eventsAfterNoop,
      undo,
      afterUndo,
      undoneSave,
      changed,
      afterChanged,
      eventsAfterChanged,
      malformedCode,
      afterMalformed,
      afterMalformedSave,
      eventsAfterMalformed,
      legacy,
      legacyResult,
      legacySaveData,
    };
  });
  expect(result.empty).toMatchObject({
    kind: "fxnode.command-log",
    schemaVersion: 2,
    composition: { id: "application-a", version: 11 },
    commands: [],
  });
  expect(result.afterQuery).toEqual(result.initial);
  expect(result.queryEvents).toEqual({ mutations: 0, snapshots: 0 });
  expect(result.populated.commands).toEqual([
    { type: "node.add", nodeId: "logged", nodeType: "alpha-node", position: { x: 12, y: 34 } },
  ]);
  expect(result.eventsAfterAdd).toEqual({ mutations: 1, snapshots: 1 });
  expect(result.noop).toEqual({ status: "noop", version: 1 });
  expect(result.afterNoop.nodes).toHaveLength(1);
  expect(result.eventsAfterNoop).toEqual(result.eventsAfterAdd);
  expect(result.undo).toEqual({ status: "committed", version: 2 });
  expect(result.afterUndo.nodes).toHaveLength(0);
  expect(result.undoneSave.commands).toEqual([]);
  expect(result.changed).toEqual({ status: "committed", version: 3 });
  expect(result.afterChanged.nodes).toEqual(result.afterNoop.nodes);
  expect(result.eventsAfterChanged).toEqual({ mutations: 3, snapshots: 3 });
  expect(result.malformedCode).toBe("node.missing");
  expect(result.afterMalformed).toEqual(result.afterChanged);
  expect(result.afterMalformedSave).toEqual(result.populated);
  expect(result.eventsAfterMalformed).toEqual(result.eventsAfterChanged);
  expect(result.legacyResult).toEqual({ status: "committed", version: 4 });
  expect(result.legacySaveData.commands).toEqual([]);
  expect(result.legacySaveData.baseline).toEqual(result.legacy);
});

test("save data carries effective composition and reports incompatible current authority", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      api = await h.createA(),
      saved = await api.getSaveData(),
      definition = { ...h.compositionA.nodes["alpha-node"], title: "Dynamic" };
    const added = await api.composeNode("dynamic-node", definition),
      loaded = await api.load(saved),
      underSuperset = await api.getSaveData();
    const removed = await api.removeNode("alpha-node", { expectedRevision: added.revision }),
      before = await api.getState();
    let error: any = {};
    try {
      await api.load(saved);
    } catch (value) {
      const e = value as any;
      error = { code: e.code, path: e.path, message: e.message, issues: e.issues };
    }
    const after = await api.getState(),
      stillCurrent = await api.getSaveData();
    api.destroy();
    return { saved, loaded, underSuperset, before, error, after, stillCurrent };
  });
  expect(result.saved.composition.nodes["alpha-node"]).toBeTruthy();
  expect(result.saved.composition.nodes["dynamic-node"]).toBeUndefined();
  expect(result.loaded.status).toBe("noop");
  expect(result.underSuperset.composition.nodes["dynamic-node"]).toBeTruthy();
  expect(result.error).toMatchObject({
    code: "composition.incompatible",
    path: "/composition",
    message: expect.stringContaining("semantic superset"),
    issues: [
      { code: "composition.incompatible" },
      { code: "composition.definition-missing", path: "/composition/nodes/alpha-node" },
    ],
  });
  expect(result.after).toEqual(result.before);
  expect(result.stillCurrent.composition.nodes["alpha-node"]).toBeUndefined();
  expect(result.stillCurrent.composition.nodes["dynamic-node"]).toBeTruthy();
});

test("dedicated workers bind disjoint application compositions and plain init sources", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const initialized = await page.evaluate(async () => {
    const harness = (
      window as unknown as {
        workerCompositionTest: {
          createA: () => Promise<any>;
          createB: () => Promise<any>;
          view: (api: any) => any;
        };
      }
    ).workerCompositionTest;
    const original = Worker.prototype.postMessage,
      initMessages: Array<{ id: string; applicationId: unknown; applicationVersion: unknown; resources: unknown }> = [],
      initResponses: Array<{ id: string; hasValue: boolean }> = [],
      initOrder: string[] = [];
    Worker.prototype.postMessage = function (this: Worker, message: unknown, transfer?: Transferable[]) {
      const value = message as {
        type?: string;
        id?: string;
        applicationId?: unknown;
        applicationVersion?: unknown;
        resources?: unknown;
      };
      if (value?.type === "init" && typeof value.id === "string") {
        initMessages.push({
          id: value.id,
          applicationId: value.applicationId,
          applicationVersion: value.applicationVersion,
          resources: value.resources,
        });
        const id = value.id;
        this.addEventListener(
          "message",
          (event: MessageEvent<unknown>) => {
            const response = event.data as any;
            if (response?.type === "response" && response.id === id && response.ok) {
              initResponses.push({ id, hasValue: "value" in response });
              initOrder.push(`${id}:response`);
            }
          },
          { once: false },
        );
      }
      return Reflect.apply(original, this, [message, transfer ?? []]);
    } as typeof Worker.prototype.postMessage;
    try {
      const [a, b] = await Promise.all([harness.createA(), harness.createB()]);
      await Promise.all([harness.view(a).whenRendered(), harness.view(b).whenRendered()]);
      const addA = await a.dispatch({ type: "node.add", nodeType: "alpha-node", position: { x: 0, y: 0 } }),
        addB = await b.dispatch({ type: "node.add", nodeType: "beta-node", position: { x: 0, y: 0 } });
      let crossA = "",
        crossB = "";
      try {
        await a.dispatch({ type: "node.add", nodeType: "beta-node", position: { x: 0, y: 0 } });
      } catch (error) {
        crossA = (error as { code?: string }).code ?? "";
      }
      try {
        await b.dispatch({ type: "node.add", nodeType: "alpha-node", position: { x: 0, y: 0 } });
      } catch (error) {
        crossB = (error as { code?: string }).code ?? "";
      }
      const beforeDestroy = {
        a: await a.getState(),
        b: await b.getState(),
        saveA: await a.save(),
        saveB: await b.save(),
      };
      a.destroy();
      const afterDestroy = await b.getState();
      const wire = initMessages.map((item) => {
        return {
          id: item.id,
          plain: Object.getPrototypeOf(item.resources as object) === Object.prototype,
          cloneable: !!structuredClone(item),
          applicationId: item.applicationId,
          applicationVersion: item.applicationVersion,
        };
      });
      b.destroy();
      return {
        addA,
        addB,
        crossA,
        crossB,
        beforeDestroy,
        afterDestroy,
        wire,
        responses: initResponses,
        initOrder,
      };
    } finally {
      Worker.prototype.postMessage = original;
    }
  });
  expect(initialized.addA).toEqual({ status: "committed", version: 1 });
  expect(initialized.addB).toEqual({ status: "committed", version: 1 });
  expect(initialized.crossA).toBe("node.type-unknown");
  expect(initialized.crossB).toBe("node.type-unknown");
  expect(initialized.beforeDestroy.a.nodes.map((node: any) => node.typeId)).toEqual(["alpha-node"]);
  expect(initialized.beforeDestroy.b.nodes.map((node: any) => node.typeId)).toEqual(["beta-node"]);
  expect(initialized.beforeDestroy.saveA.catalogVersion).toBe(11);
  expect(initialized.beforeDestroy.saveB.catalogVersion).toBe(22);
  expect(initialized.afterDestroy.nodes.map((node: any) => node.typeId)).toEqual(["beta-node"]);
  expect(initialized.wire).toHaveLength(2);
  expect(initialized.wire.every((item) => item.plain && item.cloneable)).toBe(true);
  expect(
    initialized.wire.map(({ applicationId, applicationVersion }) => ({ applicationId, applicationVersion })),
  ).toEqual([
    { applicationId: "application-a", applicationVersion: 11 },
    { applicationId: "application-b", applicationVersion: 22 },
  ]);
  expect(initialized.responses).toHaveLength(2);
  expect(initialized.responses.every((item) => !item.hasValue)).toBe(true);
  expect(initialized.initOrder.every((value: string) => value.endsWith(":response"))).toBe(true);

  const a = page.locator("#application-a"),
    b = page.locator("#application-b");
  // Recreate editors solely to verify the DOM menu remains the host's static HTML.
  await page.reload();
  await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest;
    const [a, b] = await Promise.all([h.createA(), h.createB()]);
    (window as any).menuApis = [a, b];
  });
  await a.click({ button: "right", position: { x: 20, y: 20 } });
  await expect(page.getByRole("option")).toHaveText(["Alpha Node"]);
  await page.keyboard.press("Escape");
  await b.click({ button: "right", position: { x: 20, y: 20 } });
  await expect(page.getByRole("option")).toHaveText(["Alpha Node"]);
  await page.evaluate(() => (window as any).menuApis.forEach((api: any) => api.destroy()));
});

test("value inherited from Object.prototype is never mistaken for worker response state", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest;
    Object.defineProperty(Object.prototype, "value", { configurable: true, value: { leaked: true } });
    try {
      const api = await h.createA(),
        snapshot = await api.getState();
      api.destroy();
      return { created: true, version: snapshot.version };
    } finally {
      delete (Object.prototype as any).value;
    }
  });
  expect(result).toEqual({ created: true, version: 0 });
});

test("live composition instance API keeps one worker-authoritative revisioned handle", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      NativeWorker = Worker,
      workers: Worker[] = [];
    class TrackingWorker extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        workers.push(this);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: TrackingWorker });
    const api = await h.createA(),
      compositionEvents: any[] = [],
      mutations: any[] = [],
      snapshots: any[] = [];
    api.onCompositionChanges((event: any) => compositionEvents.push(event));
    api.onMutations((event: any) => mutations.push(event));
    api.onSnapshots((event: any) => snapshots.push(event));
    const definition = { ...h.compositionA.nodes["alpha-node"], title: "Dynamic Node" };
    const composed = await api.composeNode("dynamic-node", definition, { expectedRevision: 1 }),
      sameObject = true;
    const added = await api.dispatch({
      type: "node.add",
      nodeId: "dynamic",
      nodeType: "dynamic-node",
      position: { x: 0, y: 0 },
    });
    const removed = await api.removeNode("dynamic-node", { expectedRevision: 2 }),
      afterRemoval = await api.getState(),
      undo = await api.undo();
    const themeA = { ...h.compositionA.theme, background: "#111111" },
      themeB = { ...h.compositionA.theme, background: "#222222" };
    const concurrent = await Promise.allSettled([
      api.setTheme(themeA, { expectedRevision: 3 }),
      api.setTheme(themeB, { expectedRevision: 3 }),
    ]);
    const raw = new Promise<any>((resolve) => {
      const id = "malformed-live-update",
        worker = workers[0]!;
      const listener = (event: MessageEvent<any>) => {
        if (event.data?.type === "response" && event.data.id === id) {
          worker.removeEventListener("message", listener);
          resolve(event.data);
        }
      };
      worker.addEventListener("message", listener);
      worker.postMessage({
        protocol: 3,
        type: "composition.update",
        id,
        expected: { kind: "exact", revision: -1 },
        update: { kind: "node.remove", id: "dynamic-node" },
      });
    });
    const malformed = await raw,
      stillAlive = await api.getState();
    api.destroy();
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    return {
      sameObject,
      composed,
      added,
      removed,
      afterRemoval,
      undo,
      concurrent: concurrent.map((item) =>
        item.status === "fulfilled"
          ? { status: "fulfilled", value: item.value }
          : { status: "rejected", code: (item.reason as any).code },
      ),
      compositionEvents,
      mutations,
      snapshots,
      malformed,
      stillAlive,
    };
  });
  expect(result.sameObject).toBe(true);
  expect(result.composed).toEqual({
    status: "committed",
    revision: 2,
    graphVersion: 0,
    graphChanged: false,
    historyReset: true,
  });
  expect(result.added).toEqual({ status: "committed", version: 1 });
  expect(result.removed).toEqual({
    status: "committed",
    revision: 3,
    graphVersion: 2,
    graphChanged: true,
    historyReset: true,
  });
  expect(result.afterRemoval.nodes).toHaveLength(1);
  expect(result.afterRemoval.nodes[0]).toMatchObject({ id: "dynamic", typeId: "dynamic-node", known: false });
  expect(result.undo).toEqual({ status: "noop", version: 2 });
  expect(result.concurrent.filter((item: any) => item.status === "fulfilled")).toHaveLength(1);
  expect(result.concurrent.filter((item: any) => item.status === "rejected")).toEqual([
    { status: "rejected", code: "composition.revision.stale" },
  ]);
  expect(result.compositionEvents.map((event: any) => event.revision)).toEqual([2, 3, 4]);
  expect(result.mutations).toHaveLength(2);
  expect(result.mutations[1]).toMatchObject({
    cause: "composition",
    baseVersion: 1,
    version: 2,
    mutations: [{ kind: "document.replaced" }],
  });
  expect(result.snapshots).toHaveLength(2);
  expect(result.snapshots[0].snapshot.nodes[0]).toMatchObject({ typeId: "dynamic-node", known: true });
  expect(result.malformed).toMatchObject({ ok: false, error: { code: "composition.request.invalid" } });
  expect(result.stillAlive.version).toBe(2);
});

test("canonical live-composition noops preserve revision, history, and events", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      api = await h.createA(),
      events: any[] = [];
    api.onCompositionChanges((event: any) => events.push(event));
    await api.dispatch({ type: "node.add", nodeId: "alpha", nodeType: "alpha-node", position: { x: 0, y: 0 } });
    const noop = await api.setTheme(structuredClone(h.compositionA.theme), { expectedRevision: 1 });
    const undo = await api.undo(),
      snapshot = await api.getState();
    api.destroy();
    return { noop, undo, events, snapshot };
  });
  expect(result.noop).toEqual({
    status: "noop",
    revision: 1,
    graphVersion: 1,
    graphChanged: false,
    historyReset: false,
  });
  expect(result.events).toEqual([]);
  expect(result.undo).toEqual({ status: "committed", version: 2 });
  expect(result.snapshot.nodes).toHaveLength(0);
});

test("loadComposition atomically replaces authority, checkpoints persistence, and handles prototype-named definitions", async ({
  page,
}) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      api = await h.createA(),
      compositionEvents: any[] = [],
      mutations: any[] = [],
      snapshots: any[] = [];
    api.onCompositionChanges((event: any) => compositionEvents.push(event));
    api.onMutations((event: any) => mutations.push(event));
    api.onSnapshots((event: any) => snapshots.push(event));
    const definition = { ...h.compositionA.nodes["alpha-node"], title: "Prototype Name" };
    const composed = await api.composeNode("toString", definition);
    await api.dispatch({ type: "node.add", nodeId: "alpha", nodeType: "alpha-node", position: { x: 0, y: 0 } });
    await api.dispatch({ type: "node.add", nodeId: "prototype", nodeType: "toString", position: { x: 200, y: 0 } });
    const withDefinitions = await api.getSaveData(),
      withoutDefinitions = { ...withDefinitions.composition, nodes: {} };
    const replaced = await api.loadComposition(withoutDefinitions, { expectedRevision: composed.revision }),
      demoted = await api.getState(),
      checkpoint = await api.getSaveData();
    const replay = await api.load(checkpoint),
      noop = await api.loadComposition(structuredClone(withoutDefinitions), { expectedRevision: replaced.revision }),
      undo = await api.undo();
    const presentation = { ...withoutDefinitions, theme: { ...withoutDefinitions.theme, background: "#303134" } },
      presented = await api.loadComposition(presentation, { expectedRevision: replaced.revision });
    const beforeRejected = {
      snapshot: await api.getState(),
      save: await api.getSaveData(),
      eventCount: compositionEvents.length,
      mutationCount: mutations.length,
      snapshotCount: snapshots.length,
    };
    const errors: any[] = [];
    try {
      await api.loadComposition(withDefinitions.composition, { expectedRevision: replaced.revision });
    } catch (error) {
      errors.push({ code: (error as any).code });
    }
    try {
      await api.loadComposition({ ...presentation, schemaVersion: 1 } as any, { expectedRevision: presented.revision });
    } catch (error) {
      errors.push({ code: (error as any).code });
    }
    const afterRejected = {
      snapshot: await api.getState(),
      save: await api.getSaveData(),
      eventCount: compositionEvents.length,
      mutationCount: mutations.length,
      snapshotCount: snapshots.length,
    };
    const promotedComposition = { ...presentation, nodes: { toString: definition } },
      promoted = await api.loadComposition(promotedComposition, { expectedRevision: presented.revision }),
      promotedSnapshot = await api.getState();
    api.destroy();
    return {
      composed,
      replaced,
      demoted,
      checkpoint,
      replay,
      noop,
      undo,
      presented,
      beforeRejected,
      errors,
      afterRejected,
      promoted,
      promotedSnapshot,
      compositionEvents,
      mutations,
      snapshots,
    };
  });
  expect(result.replaced).toEqual({
    status: "committed",
    revision: 3,
    graphVersion: 3,
    graphChanged: true,
    historyReset: true,
  });
  expect(result.demoted.nodes).toHaveLength(2);
  expect(result.demoted.nodes.every((node: any) => node.known === false)).toBe(true);
  expect(result.demoted.nodes.map((node: any) => node.typeId).sort()).toEqual(["alpha-node", "toString"]);
  expect(result.checkpoint.composition.nodes).toEqual({});
  expect(result.checkpoint.commands).toEqual([]);
  expect(result.checkpoint.baseline.nodes).toHaveLength(2);
  expect(result.replay).toEqual({ status: "noop", version: 3 });
  expect(result.noop).toEqual({
    status: "noop",
    revision: 3,
    graphVersion: 3,
    graphChanged: false,
    historyReset: false,
  });
  expect(result.undo).toEqual({ status: "noop", version: 3 });
  expect(result.presented).toEqual({
    status: "committed",
    revision: 4,
    graphVersion: 3,
    graphChanged: false,
    historyReset: true,
  });
  expect(result.errors).toEqual([{ code: "composition.revision.stale" }, { code: "composition.invalid" }]);
  expect(result.afterRejected).toEqual(result.beforeRejected);
  expect(result.promoted).toEqual({
    status: "committed",
    revision: 5,
    graphVersion: 4,
    graphChanged: true,
    historyReset: true,
  });
  expect(result.promotedSnapshot.nodes.find((node: any) => node.id === "prototype")).toMatchObject({
    typeId: "toString",
    known: true,
  });
  expect(result.promotedSnapshot.nodes.find((node: any) => node.id === "alpha")).toMatchObject({
    typeId: "alpha-node",
    known: false,
  });
  expect(result.compositionEvents.map((event: any) => event.change.kind)).toEqual([
    "node.compose",
    "composition.load",
    "composition.load",
    "composition.load",
  ]);
  expect(result.mutations).toHaveLength(4);
  expect(result.snapshots).toHaveLength(4);
});

test("live node composition does not alter the host-owned DOM add menu", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const canvas = page.locator("#application-a");
  await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      api = await h.createA(),
      definition = {
        ...h.compositionA.nodes["alpha-node"],
        title: "Dynamic Node",
      };
    const update = await api.composeNode("dynamic-node", definition);
    (window as any).dynamicMenu = { api, revision: update.revision };
  });
  await canvas.click({ button: "right", position: { x: 20, y: 20 } });
  await expect(page.getByRole("option")).toHaveText(["Alpha Node"]);
  await page.keyboard.press("Escape");
  await page.evaluate(async () => {
    const h = (window as any).dynamicMenu;
    await h.api.removeNode("dynamic-node", { expectedRevision: h.revision });
  });
  await canvas.click({ button: "right", position: { x: 340, y: 200 } });
  await expect(page.getByRole("option")).toHaveText(["Alpha Node"]);
  await page.keyboard.press("Escape");
  await page.evaluate(() => (window as any).dynamicMenu.api.destroy());
});

test("worker independently rejects a composition corrupted only on the init wire", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const harness = (window as any).workerCompositionTest,
      canvas = document.querySelector<HTMLCanvasElement>("#application-a")!,
      original = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, message: unknown, transfer?: Transferable[]) {
      const value = message as { type?: string };
      return Reflect.apply(original, this, [
        value?.type === "init" ? { ...(message as object), resources: { malformed: true } } : message,
        transfer ?? [],
      ]);
    } as typeof Worker.prototype.postMessage;
    let error: { name?: string; code?: string; issues?: readonly unknown[]; message?: string } = {};
    try {
      await harness.createA();
    } catch (value) {
      const e = value as typeof error;
      error = {
        ...(e.name === undefined ? {} : { name: e.name }),
        ...(e.code === undefined ? {} : { code: e.code }),
        ...(e.issues === undefined ? {} : { issues: e.issues }),
        ...(e.message === undefined ? {} : { message: e.message }),
      };
    } finally {
      Worker.prototype.postMessage = original;
    }
    return {
      error,
      tabindex: canvas.getAttribute("tabindex"),
      touchAction: canvas.style.touchAction,
      inputs: document.querySelectorAll('input[type="file"]').length,
    };
  });
  expect(result.error.name).toBe("FxNodeWorkerError");
  expect(result.error.code).toBe("composition.invalid");
  expect(result.error.issues?.length).toBeGreaterThan(0);
  expect(result.error.issues?.length).toBeLessThanOrEqual(100);
  expect(result).toMatchObject({ tabindex: null, touchAction: "", inputs: 0 });
});

test("overlapping resource tokens resolve isolated worker byte and dimension policies", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const harness = (window as any).workerCompositionTest,
      openedA: any[] = [],
      openedB: any[] = [];
    const [a, b] = await Promise.all([
        harness.createA((request: any) => openedA.push(request)),
        harness.createB((request: any) => openedB.push(request)),
      ]),
      viewA = harness.view(a),
      viewB = harness.view(b);
    await Promise.all([
      a.dispatch({ type: "node.add", nodeId: "shared", nodeType: "alpha-node", position: { x: -60, y: 30 } }),
      b.dispatch({ type: "node.add", nodeId: "shared", nodeType: "beta-node", position: { x: -60, y: 30 } }),
    ]);
    await Promise.all([
      a.dispatch({ type: "node.move", id: "shared", position: { x: -59, y: 30 } }),
      b.dispatch({ type: "node.move", id: "shared", position: { x: -59, y: 30 } }),
    ]);
    await Promise.all([viewA.whenRendered(), viewB.whenRendered()]);
    const click = async (canvas: HTMLCanvasElement, pointerId: number, opened: any[]) => {
      const rect = canvas.getBoundingClientRect(),
        before = opened.length,
        init = {
          bubbles: true,
          pointerId,
          pointerType: "mouse",
          button: 0,
          clientX: rect.left + 200,
          clientY: rect.top + 115,
        };
      canvas.dispatchEvent(new PointerEvent("pointerdown", { ...init, buttons: 1 }));
      for (let i = 0; i < 50 && opened.length === before; i++) await new Promise((resolve) => setTimeout(resolve, 10));
      canvas.dispatchEvent(new PointerEvent("pointerup", { ...init, buttons: 0 }));
    };
    await Promise.all([
      click(document.querySelector<HTMLCanvasElement>("#application-a")!, 41, openedA),
      click(document.querySelector<HTMLCanvasElement>("#application-b")!, 42, openedB),
    ]);
    const initial = [openedA[0], openedB[0]],
      accepted = initial.map((item) => item.resource.accept.join(",")),
      resourceIds = initial.map((item) => [item.resource.id]);
    const frozen = initial.every(
      (item) =>
        Object.isFrozen(item) &&
        Object.isFrozen(item.authorization) &&
        Object.isFrozen(item.resource) &&
        Object.isFrozen(item.resource.accept),
    );
    const pointerUpDidNotDuplicate = openedA.length === 1;
    const attempt = async (promise: Promise<unknown>) => {
      try {
        return { ok: true, value: await promise };
      } catch (error) {
        return { ok: false, code: (error as { code?: string }).code };
      }
    };
    const image = document.createElement("canvas");
    image.width = 2;
    image.height = 2;
    image.getContext("2d")!.fillRect(0, 0, 2, 2);
    const raw = await (await fetch(image.toDataURL("image/png"))).arrayBuffer();
    const padded = new Uint8Array(600);
    padded.set(new Uint8Array(raw));
    const dimensions = await Promise.all([
      attempt(
        viewA.provideResource(initial[0].authorization, {
          name: "pixel.png",
          mime: "image/png",
          bytes: raw.slice(0),
        }),
      ),
      attempt(
        viewB.provideResource(initial[1].authorization, {
          name: "pixel.png",
          mime: "image/png",
          bytes: raw.slice(0),
        }),
      ),
    ]);
    await Promise.all([viewA.whenRendered(), viewB.whenRendered()]);
    await Promise.all([
      click(document.querySelector<HTMLCanvasElement>("#application-a")!, 43, openedA),
      click(document.querySelector<HTMLCanvasElement>("#application-b")!, 44, openedB),
    ]);
    const next = [openedA.at(-1), openedB.at(-1)];
    const bytes = await Promise.all([
      attempt(
        viewA.provideResource(next[0].authorization, {
          name: "pixel.png",
          mime: "image/png",
          bytes: padded.buffer.slice(0),
        }),
      ),
      attempt(
        viewB.provideResource(next[1].authorization, {
          name: "pixel.png",
          mime: "image/png",
          bytes: padded.buffer.slice(0),
        }),
      ),
    ]);
    await Promise.all([viewA.whenRendered(), viewB.whenRendered()]);
    image.width = 4;
    image.height = 4;
    image.getContext("2d")!.fillRect(0, 0, 4, 4);
    await click(document.querySelector<HTMLCanvasElement>("#application-b")!, 45, openedB);
    const rebindingBytes = await (await fetch(image.toDataURL("image/png"))).arrayBuffer(),
      rebinding = attempt(
        viewB.provideResource(openedB.at(-1).authorization, {
          name: "rebind.png",
          mime: "image/png",
          bytes: rebindingBytes,
        }),
      );
    await b.dispatch({ type: "node.remove", id: "shared" });
    await b.dispatch({ type: "node.add", nodeId: "shared", nodeType: "beta-node", position: { x: -60, y: 30 } });
    const rebound = await rebinding;
    const snapshots = await Promise.all([a.getState(), b.getState()]);
    a.destroy();
    b.destroy();
    return {
      accepted,
      resourceIds,
      frozen,
      pointerUpDidNotDuplicate,
      rawBytes: raw.byteLength,
      dimensions,
      bytes,
      rebound,
      values: snapshots.map((snapshot) => (snapshot.nodes[0] as any).parameters.image.value),
      types: snapshots.map((snapshot) => snapshot.nodes[0]?.typeId),
    };
  });
  expect(result.accepted).toEqual(["image/png", "image/webp"]);
  expect(result.resourceIds).toEqual([["image"], ["image"]]);
  expect(result.frozen).toBe(true);
  expect(result.pointerUpDidNotDuplicate).toBe(true);
  expect(result.rawBytes).toBeLessThanOrEqual(512);
  expect(result.dimensions[0]).toEqual({ ok: false, code: "resource.dimensions" });
  expect(result.dimensions[1]).toMatchObject({ ok: true, value: { status: "committed", version: 3 } });
  expect(result.bytes[0]).toEqual({ ok: false, code: "resource.bytes" });
  expect(result.bytes[1]).toMatchObject({ ok: true, value: { status: "committed", version: 4 } });
  expect(result.rebound).toEqual({ ok: false, code: "resource.stale" });
  expect(result.values).toEqual(["", ""]);
  expect(result.types).toEqual(["alpha-node", "beta-node"]);
});

test("authoritative resource hits follow paint order and block click-through", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const h = (window as any).workerCompositionTest,
      opened: any[] = [],
      api = await h.createA((target: any) => opened.push(target)),
      view = h.view(api),
      canvas = document.querySelector<HTMLCanvasElement>("#application-a")!;
    await view.addNode({ typeId: "alpha-node", nodeId: "bottom", viewPosition: { x: 120, y: 80 } });
    await view.addNode({ typeId: "alpha-node", nodeId: "top", viewPosition: { x: 120, y: 80 } });
    await view.whenRendered();
    const rect = canvas.getBoundingClientRect(),
      click = async (pointerId: number) => {
        const before = opened.length;
        const init = {
          bubbles: true,
          pointerId,
          pointerType: "mouse",
          button: 0,
          clientX: rect.left + 200,
          clientY: rect.top + 115,
        };
        canvas.dispatchEvent(new PointerEvent("pointerdown", { ...init, buttons: 1 }));
        for (let i = 0; i < 50 && opened.length === before; i++)
          await new Promise((resolve) => setTimeout(resolve, 10));
        canvas.dispatchEvent(new PointerEvent("pointerup", { ...init, buttons: 0 }));
      };
    await click(71);
    const topToken = opened[0]?.authorization.token;
    const blocker = {
      ...h.compositionA.nodes["alpha-node"],
      title: "Blocker",
      parameters: {},
      ui: [
        { kind: "socket", socket: "output" },
        ...Array.from({ length: 7 }, (_, index) => ({ kind: "text", variant: "section", title: `Row ${index}` })),
      ],
    };
    await api.composeNode("blocker", blocker);
    await view.addNode({ typeId: "blocker", nodeId: "cover", viewPosition: { x: 120, y: 80 } });
    await view.whenRendered();
    await click(72);
    const blocked = opened.length === 1;
    await api.dispatch({ type: "node.remove", id: "cover" });
    await view.whenRendered();
    h.destroy(api);
    return { topToken, blocked };
  });
  expect(result).toEqual({ topToken: expect.stringMatching(/^top:/), blocked: true });
});

test("delayed resource requests survive pointer-up but not newer host intent", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const harness = (window as any).workerCompositionTest,
      NativeWorker = Worker;
    class DelayedWorker extends NativeWorker {
      static hold = false;
      static held: { worker: DelayedWorker; event: MessageEvent } | undefined;
      private receiver: ((this: Worker, event: MessageEvent) => any) | null = null;
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        this.addEventListener("message", (event) => {
          if (DelayedWorker.hold && (event.data as { type?: unknown })?.type === "view.resource.open") {
            DelayedWorker.hold = false;
            DelayedWorker.held = { worker: this, event };
            return;
          }
          this.receiver?.call(this, event);
        });
      }
      override set onmessage(value: ((this: Worker, event: MessageEvent) => any) | null) {
        this.receiver = value;
      }
      override get onmessage() {
        return this.receiver;
      }
      static release() {
        const held = DelayedWorker.held;
        DelayedWorker.held = undefined;
        held?.worker.receiver?.call(held.worker, held.event);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: DelayedWorker });
    let api: any;
    try {
      const opened: any[] = [];
      api = await harness.createA((request: any) => opened.push(request));
      const view = harness.view(api);
      await view.addNode({ typeId: "alpha-node", nodeId: "resource", viewPosition: { x: 120, y: 80 } });
      await view.whenRendered();
      const canvas = document.querySelector<HTMLCanvasElement>("#application-a")!,
        rect = canvas.getBoundingClientRect(),
        click = (pointerId: number) => {
          const init = {
            bubbles: true,
            pointerId,
            pointerType: "mouse",
            button: 0,
            clientX: rect.left + 200,
            clientY: rect.top + 115,
          };
          canvas.dispatchEvent(new PointerEvent("pointerdown", { ...init, buttons: 1 }));
          canvas.dispatchEvent(new PointerEvent("pointerup", { ...init, buttons: 0 }));
        },
        waitFor = async (predicate: () => boolean) => {
          for (let index = 0; index < 100; index++) {
            if (predicate()) return true;
            await new Promise(requestAnimationFrame);
          }
          return false;
        };
      DelayedWorker.hold = true;
      click(91);
      const heldThroughPointerUp = await waitFor(() => !!DelayedWorker.held);
      DelayedWorker.release();
      const deliveredAfterPointerUp = await waitFor(() => opened.length === 1);

      DelayedWorker.hold = true;
      click(92);
      const heldBeforeNewIntent = await waitFor(() => !!DelayedWorker.held);
      view.feedInput({
        kind: "wheel",
        position: { x: 200, y: 115 },
        delta: { x: 0, y: -20 },
        modifiers: { alt: false, control: false, meta: false, shift: false },
      });
      DelayedWorker.release();
      await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      return {
        heldThroughPointerUp,
        deliveredAfterPointerUp,
        heldBeforeNewIntent,
        staleIgnored: opened.length === 1,
      };
    } finally {
      if (api) harness.destroy(api);
      Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    }
  });
  expect(result).toEqual({
    heldThroughPointerUp: true,
    deliveredAfterPointerUp: true,
    heldBeforeNewIntent: true,
    staleIgnored: true,
  });
});

test("an ordinary graph commit invalidates delayed add-menu authorization", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const harness = (window as any).workerCompositionTest,
      NativeWorker = Worker;
    class DelayedMenuWorker extends NativeWorker {
      static held: { worker: DelayedMenuWorker; event: MessageEvent } | undefined;
      private receiver: ((this: Worker, event: MessageEvent) => any) | null = null;
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        this.addEventListener("message", (event) => {
          if (!DelayedMenuWorker.held && (event.data as { type?: unknown })?.type === "view.node-menu.result") {
            DelayedMenuWorker.held = { worker: this, event };
            return;
          }
          this.receiver?.call(this, event);
        });
      }
      override set onmessage(value: ((this: Worker, event: MessageEvent) => any) | null) {
        this.receiver = value;
      }
      override get onmessage() {
        return this.receiver;
      }
      static release() {
        const held = DelayedMenuWorker.held;
        DelayedMenuWorker.held = undefined;
        held?.worker.receiver?.call(held.worker, held.event);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: DelayedMenuWorker });
    let api: any;
    try {
      api = await harness.createA();
      const view = harness.view(api);
      let opened = 0;
      view.onHostRequests((request: { kind: string }) => {
        if (request.kind === "add-node-menu") opened++;
      });
      const canvas = document.querySelector<HTMLCanvasElement>("#application-a")!,
        rect = canvas.getBoundingClientRect();
      canvas.dispatchEvent(
        new PointerEvent("pointerdown", {
          bubbles: true,
          pointerId: 101,
          pointerType: "mouse",
          button: 2,
          buttons: 2,
          clientX: rect.left + 20,
          clientY: rect.top + 20,
        }),
      );
      for (let index = 0; index < 100 && !DelayedMenuWorker.held; index++) await new Promise(requestAnimationFrame);
      const held = !!DelayedMenuWorker.held;
      await view.addNode({ typeId: "alpha-node", nodeId: "new-node", viewPosition: { x: 300, y: 180 } });
      DelayedMenuWorker.release();
      await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      return { held, opened, menuOpen: !!document.querySelector("[data-fxnode-add-menu]") };
    } finally {
      if (api) harness.destroy(api);
      Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    }
  });
  expect(result).toEqual({ held: true, opened: 0, menuOpen: false });
});

test("invalid bootstrap authority rejects before constructing a worker", async ({ page }) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const { createFxNode } = await import("@lib/index.js"),
      canvas = document.createElement("canvas"),
      NativeWorker = Worker;
    let constructed = 0;
    class TrackingWorker extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        constructed++;
        super(url, options);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: TrackingWorker });
    const errors: string[] = [];
    try {
      for (const authority of [
        { applicationId: "", applicationVersion: 1, resources: {} },
        { applicationId: "valid", applicationVersion: 0, resources: {} },
        { applicationId: "valid", applicationVersion: 1, resources: { broken: { kind: "image" } } },
      ])
        try {
          await createFxNode({
            applicationId: authority.applicationId,
            applicationVersion: authority.applicationVersion,
            resources: authority.resources as never,
          });
        } catch (error) {
          errors.push((error as Error).name);
        }
      return { constructed, errors };
    } finally {
      Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    }
  });
  expect(result).toEqual({
    constructed: 0,
    errors: ["FxNodeCompositionError", "FxNodeCompositionError", "FxNodeCompositionError"],
  });
});

test("startup attachment failure terminates the worker and invalid composition has no side effects", async ({
  page,
}) => {
  await page.goto("/test/browser/worker-composition.html");
  const result = await page.evaluate(async () => {
    const harness = (window as any).workerCompositionTest,
      canvas = document.querySelector<HTMLCanvasElement>("#application-a")!,
      NativeWorker = Worker,
      NativeObserver = ResizeObserver;
    let constructed = 0,
      terminated = 0;
    class TrackingWorker extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        constructed++;
      }
      override terminate() {
        terminated++;
        super.terminate();
      }
    }
    class ThrowingObserver {
      constructor(_callback: ResizeObserverCallback) {}
      observe() {
        throw new Error("observe exploded");
      }
      disconnect() {}
      unobserve() {}
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: TrackingWorker });
    Object.defineProperty(window, "ResizeObserver", { configurable: true, writable: true, value: ThrowingObserver });
    canvas.setAttribute("tabindex", "7");
    canvas.style.touchAction = "pan-x";
    let attachMessage = "";
    try {
      await harness.createA();
    } catch (error) {
      attachMessage = (error as Error).message;
    }
    const attached = {
      constructed,
      terminated,
      tabindex: canvas.getAttribute("tabindex"),
      touchAction: canvas.style.touchAction,
      inputs: document.querySelectorAll('input[type="file"]').length,
    };
    Object.defineProperty(window, "ResizeObserver", { configurable: true, writable: true, value: NativeObserver });
    let invalidMessage = "";
    try {
      await harness.createRaw(canvas, { malformed: true });
    } catch (error) {
      invalidMessage = (error as Error).message;
    }
    const invalid = {
      constructed,
      terminated,
      tabindex: canvas.getAttribute("tabindex"),
      touchAction: canvas.style.touchAction,
      inputs: document.querySelectorAll('input[type="file"]').length,
    };
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    return { attachMessage, attached, invalidMessage, invalid };
  });
  expect(result.attachMessage).toBe("observe exploded");
  expect(result.attached).toEqual({ constructed: 1, terminated: 1, tabindex: "7", touchAction: "pan-x", inputs: 0 });
  expect(result.invalidMessage).toContain("Invalid fxnode composition");
  expect(result.invalid).toEqual({ constructed: 2, terminated: 2, tabindex: "7", touchAction: "pan-x", inputs: 0 });
});
