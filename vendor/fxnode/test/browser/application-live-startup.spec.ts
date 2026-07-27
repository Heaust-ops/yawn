import { expect, test } from "@playwright/test";

const expectedNodeTypes = [
  "fxnode.common.frame",
  "fxnode.common.reroute",
  "fxnode.common.group-input",
  "fxnode.common.group-output",
  "fxnode.shader.value",
  "fxnode.shader.color",
  "fxnode.shader.math",
  "fxnode.shader.vector-math",
  "fxnode.shader.mix",
  "fxnode.shader.color-ramp",
  "fxnode.shader.texture-coordinate",
  "fxnode.shader.noise-texture",
  "fxnode.shader.image-texture",
  "fxnode.shader.principled-bsdf",
  "fxnode.shader.material-output",
  "fxnode.geometry.position",
  "fxnode.geometry.mesh-cube",
  "fxnode.geometry.set-position",
  "fxnode.geometry.transform-geometry",
  "fxnode.geometry.join-geometry",
  "fxnode.compositor.image",
  "fxnode.compositor.color-balance",
] as const;

test("example installs every startup node through the live worker API", async ({ page }) => {
  await page.addInitScript(() => {
    const sent: unknown[] = [],
      received: unknown[] = [];
    (
      window as unknown as { applicationStartupMessages: { sent: unknown[]; received: unknown[] } }
    ).applicationStartupMessages = { sent, received };
    const NativeWorker = Worker;
    class StartupWorker extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        super.addEventListener("message", (event) => received.push(structuredClone(event.data)));
      }
      override postMessage(message: unknown, options?: StructuredSerializeOptions | Transferable[]) {
        sent.push(structuredClone(message));
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: StartupWorker });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const result = await page.evaluate(async () => {
    const messages = (window as unknown as { applicationStartupMessages: { sent: any[]; received: any[] } })
        .applicationStartupMessages,
      init = messages.sent.find((message) => message.type === "init"),
      updates = messages.sent.filter((message) => message.type === "composition.update"),
      stateSet = messages.sent.find((message) => message.type === "state.set"),
      api = window.fxnodeExample.root!;
    const updateIds = new Set(updates.map((message) => message.id)),
      compositionReceipts = messages.received
        .filter((message) => message.type === "response" && updateIds.has(message.id))
        .map((message) => message.value),
      stateSetIndex = messages.sent.indexOf(stateSet),
      lastUpdateIndex = Math.max(...updates.map((message) => messages.sent.indexOf(message))),
      snapshot = await api.getState(),
      undo = await api.undo({ expectedVersion: 1 }),
      empty = await api.getState(),
      redo = await api.redo({ expectedVersion: 2 }),
      restored = await api.getState();
    return {
      initKeys: Object.keys(init).sort(),
      updates: updates.map((message) => ({
        kind: message.update.kind,
        id: message.update.id,
        expected: message.expected,
      })),
      compositionReceipts,
      stateSet,
      stateSetIndex,
      lastUpdateIndex,
      snapshot,
      undo,
      empty,
      redo,
      restored,
      saved: await api.save(),
      initialLayout: await (await fetch("/examples/blender/initialLayout.json")).json(),
    };
  });
  expect(result.initKeys).toEqual([
    "applicationId",
    "applicationVersion",
    "historyLimit",
    "id",
    "protocol",
    "resources",
    "type",
  ]);
  expect(result.updates).toHaveLength(31);
  expect(result.updates.slice(0, 9).map((update) => update.kind)).toEqual([
    "theme.set",
    "header-styles.set",
    ...Array(6).fill("socket.compose"),
    "compatibility.set",
  ]);
  expect(result.updates.slice(9).map((update) => update.kind)).toEqual(Array(22).fill("node.compose"));
  expect(result.updates.slice(9).map((update) => update.id)).toEqual(expectedNodeTypes);
  expect(result.updates.map((update) => update.expected)).toEqual(Array(31).fill({ kind: "current" }));
  expect(result.compositionReceipts).toHaveLength(31);
  expect(
    result.compositionReceipts.every((receipt) => receipt.graphVersion === 0 && receipt.graphChanged === false),
  ).toBe(true);
  expect(result.stateSetIndex).toBeGreaterThan(result.lastUpdateIndex);
  expect(result.stateSet.expected).toEqual({ kind: "current" });
  expect(result.stateSet.state.schemaVersion).toBeUndefined();
  expect(result.stateSet.state.version).toBeUndefined();
  expect(result.stateSet.state.nodes.every((node: any) => node.known === true)).toBe(true);
  expect(result.snapshot.version).toBe(1);
  expect(result.snapshot.nodes.length).toBe(result.initialLayout.nodes.length);
  expect(result.snapshot.nodes.every((node: any) => node.known)).toBe(true);
  expect(result.undo).toEqual({ status: "committed", version: 2 });
  expect(result.empty).toMatchObject({ version: 2, nodes: [], links: [] });
  expect(result.redo).toEqual({ status: "committed", version: 3 });
  expect(result.restored.nodes).toEqual(result.snapshot.nodes);
  expect(result.restored.links).toEqual(result.snapshot.links);
  expect(result.saved).toEqual({
    schemaVersion: 2,
    ...result.initialLayout,
    nodes: result.initialLayout.nodes.map(({ known: _known, ...node }: any) => node),
  });
});
