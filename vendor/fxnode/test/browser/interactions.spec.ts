import { expect, test } from "@playwright/test";

test("direct input preserves modifiers, SAB ordering, and right-button menu rules", async ({ page }) => {
  await page.addInitScript(() => {
    const Native = Worker;
    const messages: unknown[] = [];
    let lane: SharedArrayBuffer | undefined;
    class Spy extends Native {
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        const value = message as { type?: string; pointerLane?: SharedArrayBuffer };
        if (value.type === "view.attach") lane = value.pointerLane;
        if (["view.input", "view.pointer.flush", "view.viewport", "view.render"].includes(value.type ?? ""))
          messages.push(message);
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperties(window, { phase1Messages: { value: messages }, phase1Lane: { get: () => lane } });
    Object.defineProperty(window, "Worker", { value: Spy });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const result = await page.evaluate(async () => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      w = window as unknown as { phase1Messages: any[]; phase1Lane?: SharedArrayBuffer };
    w.phase1Messages.length = 0;
    const mods0 = { alt: false, control: false, meta: false, shift: false },
      mods15 = { alt: true, control: true, meta: true, shift: true };
    const before = api.getState();
    view.feedInput({
      kind: "pointer",
      phase: "down",
      pointerId: 9,
      pointerType: "mouse",
      position: { x: 380, y: 160 },
      button: 0,
      buttons: 1,
      modifiers: mods0,
    });
    view.feedInput({
      kind: "pointer",
      phase: "move",
      pointerId: 9,
      pointerType: "mouse",
      position: { x: 410, y: 180 },
      button: 0,
      buttons: 1,
      modifiers: mods15,
    });
    view.feedInput({
      kind: "pointer",
      phase: "up",
      pointerId: 9,
      pointerType: "mouse",
      position: { x: 410, y: 180 },
      button: 0,
      buttons: 0,
      modifiers: mods0,
    });
    await view.whenRendered();
    const after = await api.getState();
    const invalidBefore = w.phase1Messages.length,
      laneBefore = w.phase1Lane ? Array.from(new Int32Array(w.phase1Lane)) : [];
    let invalid = "";
    try {
      view.feedInput({ kind: "wheel", position: { x: 0, y: 0 }, delta: { x: NaN, y: 0 }, modifiers: mods0 });
    } catch (e) {
      invalid = (e as Error).name;
    }
    const invalidAfter = w.phase1Messages.length,
      laneAfter = w.phase1Lane ? Array.from(new Int32Array(w.phase1Lane)) : [];
    w.phase1Messages.length = 0;
    view.feedInput({
      kind: "pointer",
      phase: "move",
      pointerId: 10,
      pointerType: "mouse",
      position: { x: -3, y: -4 },
      button: 0,
      buttons: 0,
      modifiers: mods15,
    });
    await view.setViewport({ width: 301, height: 179, dpr: 1 });
    const ordered = structuredClone(w.phase1Messages);
    w.phase1Messages.length = 0;
    const rendered = view.whenRendered();
    await rendered;
    const renderMessage = w.phase1Messages.find((x) => x.type === "view.render");
    w.phase1Messages.length = 0;
    view.feedInput({
      kind: "pointer",
      phase: "down",
      pointerId: 11,
      pointerType: "mouse",
      position: { x: 1, y: 1 },
      button: 2,
      buttons: 3,
      modifiers: mods0,
    });
    const rmb = structuredClone(w.phase1Messages.at(-1));
    return {
      before: await before,
      after,
      invalid,
      invalidBefore,
      invalidAfter,
      laneBefore,
      laneAfter,
      ordered,
      renderedWithDedicatedMessage: !!renderMessage,
      rmb,
    };
  });
  expect(result.after.nodes.find((n) => n.id === "math")?.position).toEqual({ x: -270, y: 150 });
  const inputs = result.ordered.length;
  expect(result.invalid).toBe("TypeError");
  expect(result.invalidAfter).toBe(result.invalidBefore);
  expect(result.laneAfter).toEqual(result.laneBefore);
  expect(result.ordered.map((x: any) => x.type)).toEqual(["view.viewport"]);
  expect(result.ordered[0].pointerFence.before.event.modifiers).toBe(15);
  expect(result.ordered[0].viewport).toEqual({ width: 301, height: 179, dpr: 1 });
  expect(result.renderedWithDedicatedMessage).toBe(true);
  const sentMasks = (result as any).rmb.event.modifiers;
  expect(sentMasks).toBe(0);
  expect((result as any).rmb.nodeMenuRequestId).toBeUndefined();
  expect(inputs).toBe(1);
});

test("direct input invalidates a pending add-menu request", async ({ page }) => {
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  await page.evaluate(() => {
    const api = window.fxnodeExample.root!,
      view = window.fxnodeExample.view!,
      modifiers = { alt: false, control: false, meta: false, shift: false };
    view.feedInput({
      kind: "pointer",
      phase: "down",
      pointerId: 31,
      pointerType: "mouse",
      position: { x: 20, y: 20 },
      button: 2,
      buttons: 2,
      modifiers,
    });
    view.feedInput({ kind: "wheel", position: { x: 20, y: 20 }, delta: { x: 0, y: 1 }, modifiers });
  });
  await page.waitForTimeout(100);
  expect(await page.locator("[data-fxnode-add-menu]").count()).toBe(0);
});

test("synchronous input and viewport postMessage failures throw", async ({ page }) => {
  await page.addInitScript(() => {
    const Native = Worker;
    let fail = "";
    class Spy extends Native {
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        if ((message as any)?.type === fail) throw new DOMException("blocked", "DataCloneError");
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperty(window, "failWorkerPost", { set: (v: string) => (fail = v) });
    Object.defineProperty(window, "Worker", { value: Spy });
  });
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  expect(
    await page.evaluate(() => {
      (window as any).failWorkerPost = "view.input";
      try {
        window.view.feedInput({ kind: "focus", phase: "focus" });
        return "none";
      } catch (e) {
        return (e as Error).name;
      }
    }),
  ).toBe("FxNodeProtocolError");
  await page.reload();
  await page.evaluate(() => window.ready);
  expect(
    await page.evaluate(async () => {
      (window as any).failWorkerPost = "view.viewport";
      try {
        await window.view.setViewport({ width: 10, height: 10, dpr: 1 });
        return "none";
      } catch (e) {
        return (e as Error).name;
      }
    }),
  ).toBe("FxNodeProtocolError");
});

test("dedicated actions and resource transfers embed transactional SAB fences", async ({ page }) => {
  await page.addInitScript(() => {
    const Native = Worker,
      messages: unknown[] = [],
      resourceTransfers: boolean[] = [];
    let lane: SharedArrayBuffer | undefined,
      fail = "";
    class Spy extends Native {
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        const value = message as { type?: string; pointerLane?: SharedArrayBuffer; resource?: { bytes?: ArrayBuffer } };
        if (value.type === "view.attach") lane = value.pointerLane;
        if (
          ["view.node.add", "view.selection.mute", "view.selection.remove", "view.resource.set"].includes(
            value.type ?? "",
          )
        )
          messages.push(structuredClone(message));
        if (value.type === "view.resource.set")
          resourceTransfers.push(Array.isArray(options) && options[0] === value.resource?.bytes);
        if (value.type === fail) throw new DOMException("blocked", "DataCloneError");
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperties(window, {
      actionMessages: { value: messages },
      actionLane: { get: () => lane },
      resourceTransfers: { value: resourceTransfers },
      failActionPost: { set: (value: string) => (fail = value) },
    });
    Object.defineProperty(window, "Worker", { value: Spy });
  });
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const api = window.root,
      view = window.view,
      w = window as any,
      modifiers = { alt: false, control: false, meta: false, shift: false },
      move = (x: number) =>
        view.feedInput({
          kind: "pointer",
          phase: "move",
          pointerId: 7,
          pointerType: "mouse",
          position: { x, y: x + 1 },
          button: 0,
          buttons: 0,
          modifiers,
        });
    move(1);
    await view.addNode({ typeId: "fxnode.shader.value", viewPosition: { x: 40, y: 40 }, nodeId: "fenced" });
    move(2);
    await view.setSelectedMuted(true);
    move(3);
    await view.removeSelected();
    move(4);
    const transferred = new ArrayBuffer(1);
    let stale = "";
    try {
      await view.provideResource(
        { token: "missing", viewId: view.id, graphVersion: 4, compositionRevision: 30 },
        { name: "x.png", mime: "image/png", bytes: transferred },
      );
    } catch (error) {
      stale = (error as any).code;
    }
    const laneBefore = Array.from(new Int32Array(w.actionLane));
    move(5);
    const beforeFailure = Array.from(new Int32Array(w.actionLane)),
      failedBuffer = new ArrayBuffer(1);
    w.failActionPost = "view.resource.set";
    let failure = "";
    try {
      await view.provideResource(
        { token: "missing", viewId: view.id, graphVersion: 4, compositionRevision: 30 },
        { name: "x.png", mime: "image/png", bytes: failedBuffer },
      );
    } catch (error) {
      failure = (error as Error).name;
    }
    const sent = structuredClone(w.actionMessages),
      afterFailure = Array.from(new Int32Array(w.actionLane));
    return {
      sent,
      laneBefore,
      beforeFailure,
      afterFailure,
      failure,
      stale,
      detached: transferred.byteLength,
      failedBytes: failedBuffer.byteLength,
      transfers: w.resourceTransfers,
    };
  });
  expect(result.sent.map((message: any) => message.type)).toEqual([
    "view.node.add",
    "view.selection.mute",
    "view.selection.remove",
    "view.resource.set",
    "view.resource.set",
  ]);
  const generations = result.sent.map((message: any) => message.pointerFence.generation),
    base = generations[0];
  expect(generations).toEqual([base, base + 1, base + 2, base + 3, base + 4]);
  expect(result.sent.map((message: any) => message.pointerFence.before.event.position.x)).toEqual([1, 2, 3, 4, 5]);
  expect(result.laneBefore[1]).toBe(base + 3);
  expect(result.beforeFailure[1]).toBe(base + 3);
  expect(result.afterFailure[1]).toBe(base + 3);
  expect(result.failure).toBe("FxNodeProtocolError");
  expect(result.stale).toBe("resource.stale");
  expect(result.detached).toBe(0);
  expect(result.failedBytes).toBe(1);
  expect(result.transfers).toEqual([true, true]);
});

test("selection publication does not wait for frame consumption", async ({ page }) => {
  await page.addInitScript(() => {
    const Native = Worker;
    let block = false;
    class Spy extends Native {
      override postMessage(message: unknown, options?: Transferable[] | StructuredSerializeOptions) {
        if ((message as any)?.type === "view.frame.consumed" && block) return;
        super.postMessage(message, options as StructuredSerializeOptions);
      }
    }
    Object.defineProperty(window, "blockFrameConsumption", { set: (value: boolean) => (block = value) });
    Object.defineProperty(window, "Worker", { value: Spy });
  });
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    (window as any).blockFrameConsumption = true;
    const api = window.root,
      view = window.view,
      before = view.getHostSnapshot();
    const receipt = await view.addNode({
        typeId: "fxnode.shader.value",
        viewPosition: { x: 40, y: 40 },
        nodeId: "blocked-frame",
      }),
      after = view.getHostSnapshot();
    return { receipt, changed: before !== after, selection: structuredClone(after.selection) };
  });
  expect(result).toEqual({
    receipt: { status: "committed", version: 2 },
    changed: true,
    selection: { nodeCount: 1, linkCount: 0, canRemove: true, mute: { enabled: true, state: "all-unmuted" } },
  });
});

test("worker gestures stay transient and commit exactly one paired event", async ({ page }) => {
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    let moves = 0;
    class TrackedWorker extends NativeWorker {
      override postMessage(message: unknown, transfer: Transferable[]): void;
      override postMessage(message: unknown, options?: StructuredSerializeOptions): void;
      override postMessage(message: unknown, transferOrOptions?: Transferable[] | StructuredSerializeOptions): void {
        if (
          typeof message === "object" &&
          message !== null &&
          (message as { type?: unknown }).type === "view.input" &&
          (message as { event?: { kind?: unknown; phase?: unknown } }).event?.kind === "pointer" &&
          (message as { event?: { phase?: unknown } }).event?.phase === "move"
        )
          moves++;
        if (Array.isArray(transferOrOptions)) super.postMessage(message, transferOrOptions);
        else super.postMessage(message, transferOrOptions);
      }
    }
    Object.defineProperty(window, "Worker", { value: TrackedWorker });
    Object.defineProperty(window, "pointerMoveMessages", { get: () => moves });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
  await page.evaluate(() => {
    const h = window.fxnodeExample;
    const w = window as typeof window & { gestureEvents: { m: number; s: number } };
    w.gestureEvents = { m: 0, s: 0 };
    h.root!.onMutations(() => w.gestureEvents.m++);
    h.root!.onSnapshots(() => w.gestureEvents.s++);
  });
  const canvas = page.locator("#graph"),
    snapshot = () => page.evaluate(() => window.fxnodeExample.root!.getState());
  const original = await snapshot();
  // Math header is deterministic: world (-300,170), view origin (600,320).
  await canvas.click({ position: { x: 380, y: 160 } });
  expect((await snapshot()).version).toBe(original.version);
  expect(
    await page.evaluate(() => (window as typeof window & { gestureEvents: { m: number; s: number } }).gestureEvents),
  ).toEqual({ m: 0, s: 0 });
  const bounds = await canvas.boundingBox();
  if (!bounds) throw new Error("canvas bounds missing");
  await canvas.hover({ position: { x: 380, y: 160 } });
  await page.mouse.down();
  await page.mouse.move(bounds.x + 410, bounds.y + 180, { steps: 4 });
  expect((await snapshot()).nodes.find((n) => n.id === "math")?.position).toEqual({ x: -300, y: 170 });
  await page.mouse.up();
  expect((await snapshot()).nodes.find((n) => n.id === "math")?.position).toEqual({ x: -270, y: 150 });
  expect(
    await page.evaluate(() => (window as typeof window & { gestureEvents: { m: number; s: number } }).gestureEvents),
  ).toEqual({ m: 1, s: 1 });
  expect(
    await page.evaluate(() => (window as typeof window & { pointerMoveMessages: number }).pointerMoveMessages),
  ).toBe(0);
  // G and ordinary drag cancel without mutation; RMB is suppressed by the canvas.
  await canvas.press("g");
  await page.mouse.move(bounds.x + 390, bounds.y + 210);
  await canvas.press("Escape");
  expect((await snapshot()).version).toBe(original.version + 1);
  await canvas.hover({ position: { x: 390, y: 180 } });
  await page.mouse.down();
  await page.mouse.move(bounds.x + 430, bounds.y + 220);
  await canvas.press("Escape");
  await page.mouse.up();
  await canvas.click({ position: { x: 30, y: 30 }, button: "right" });
  expect((await snapshot()).version).toBe(original.version + 1);
  // Box, MMB, wheel and Home are view/selection-only.
  await canvas.hover({ position: { x: 20, y: 20 } });
  await page.mouse.down();
  await page.mouse.move(bounds.x + 250, bounds.y + 300);
  await page.mouse.up();
  await canvas.hover({ position: { x: 30, y: 30 } });
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(bounds.x + 40, bounds.y + 40);
  await page.mouse.up({ button: "middle" });
  await page.mouse.wheel(0, 30);
  await canvas.press("Home");
  expect((await snapshot()).version).toBe(original.version + 1);
});

test("non-isolated hosts retain the ordered pointer message fallback", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "crossOriginIsolated", { value: false });
    const NativeWorker = window.Worker;
    let moves = 0;
    class TrackedWorker extends NativeWorker {
      override postMessage(message: unknown, transfer: Transferable[]): void;
      override postMessage(message: unknown, options?: StructuredSerializeOptions): void;
      override postMessage(message: unknown, transferOrOptions?: Transferable[] | StructuredSerializeOptions): void {
        if (
          typeof message === "object" &&
          message !== null &&
          (message as { type?: unknown }).type === "view.input" &&
          (message as { event?: { kind?: unknown; phase?: unknown } }).event?.kind === "pointer" &&
          (message as { event?: { phase?: unknown } }).event?.phase === "move"
        )
          moves++;
        if (Array.isArray(transferOrOptions)) super.postMessage(message, transferOrOptions);
        else super.postMessage(message, transferOrOptions);
      }
    }
    Object.defineProperty(window, "Worker", { value: TrackedWorker });
    Object.defineProperty(window, "pointerMoveMessages", { get: () => moves });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
  const canvas = page.locator("#graph"),
    bounds = await canvas.boundingBox();
  if (!bounds) throw new Error("canvas bounds missing");
  await canvas.hover({ position: { x: 380, y: 160 } });
  await page.mouse.down();
  await page.mouse.move(bounds.x + 410, bounds.y + 180, { steps: 4 });
  await page.mouse.up();
  expect(
    (await page.evaluate(() => window.fxnodeExample.root!.getState())).nodes.find((node) => node.id === "math")
      ?.position,
  ).toEqual({ x: -270, y: 150 });
  expect(
    await page.evaluate(() => (window as typeof window & { pointerMoveMessages: number }).pointerMoveMessages),
  ).toBeGreaterThan(0);
});

test("wheel input visibly zooms around the pointer without changing graph state", async ({ page }) => {
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    let wheels = 0;
    class TrackedWorker extends NativeWorker {
      override postMessage(message: unknown, transfer: Transferable[]): void;
      override postMessage(message: unknown, options?: StructuredSerializeOptions): void;
      override postMessage(message: unknown, transferOrOptions?: Transferable[] | StructuredSerializeOptions): void {
        if (
          typeof message === "object" &&
          message !== null &&
          (message as { type?: unknown }).type === "view.input" &&
          (message as { event?: { kind?: unknown } }).event?.kind === "wheel"
        )
          wheels++;
        if (Array.isArray(transferOrOptions)) super.postMessage(message, transferOrOptions);
        else super.postMessage(message, transferOrOptions);
      }
    }
    Object.defineProperty(window, "Worker", { value: TrackedWorker });
    Object.defineProperty(window, "wheelMessages", { get: () => wheels });
  });
  await page.goto("/examples/blender/");
  await page.evaluate(() => window.fxnodeExample.ready);
  const canvas = page.locator("#graph"),
    before = await canvas.screenshot(),
    version = (await page.evaluate(() => window.fxnodeExample.root!.getState())).version;
  await canvas.hover({ position: { x: 400, y: 240 } });
  await page.mouse.wheel(0, -240);
  await expect
    .poll(() => page.evaluate(() => (window as typeof window & { wheelMessages: number }).wheelMessages))
    .toBe(1);
  await page.evaluate(() => window.fxnodeExample.view!.whenRendered());
  const zoomed = await canvas.screenshot();
  expect(zoomed.equals(before)).toBe(false);
  expect((await page.evaluate(() => window.fxnodeExample.root!.getState())).version).toBe(version);
  await page.mouse.wheel(0, 240);
  await page.evaluate(() => window.fxnodeExample.view!.whenRendered());
  expect((await canvas.screenshot()).equals(zoomed)).toBe(false);
});
