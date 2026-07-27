import { expect, test } from "@playwright/test";

test("canvas-free authority owns independent attachable views", async ({ page }) => {
  await page.goto("/test/browser/client-runtime.html");
  const result = await page.evaluate(async () => {
    type Wire = Record<string, unknown> & { type: string; id?: string; viewId?: string };
    class FakeWorker {
      static readonly instances: FakeWorker[] = [];
      readonly posted: Wire[] = [];
      onmessage: ((event: MessageEvent<unknown>) => void) | null = null;
      onerror: ((event: ErrorEvent) => void) | null = null;
      onmessageerror: ((event: MessageEvent<unknown>) => void) | null = null;
      terminated = false;
      holdViewports = false;
      readonly heldViewportIds: string[] = [];

      constructor() {
        FakeWorker.instances.push(this);
      }

      postMessage(message: Wire): void {
        this.posted.push(message);
        if (!message.id) return;
        if (message.type === "init") this.respond(message.id);
        else if (message.type === "view.attach") {
          // Exercise the valid event-before-acknowledgement ordering.
          this.emit({
            protocol: 3,
            type: "view.selection.host",
            viewId: message.viewId,
            projection: { nodeCount: 0, linkCount: 0, canRemove: false, mute: { enabled: false } },
          });
          this.respond(message.id);
        } else if (message.type === "view.detach") this.respond(message.id);
        else if (message.type === "command") this.respond(message.id, { status: "committed", version: 1 });
        else if (message.type === "view.viewport") {
          if (this.holdViewports) this.heldViewportIds.push(message.id);
          else this.respond(message.id);
        } else if (message.type.startsWith("view.")) this.respond(message.id, { status: "noop", version: 0 });
      }

      terminate(): void {
        this.terminated = true;
      }

      emit(data: unknown): void {
        queueMicrotask(() => this.onmessage?.(new MessageEvent("message", { data })));
      }

      settleViewport(ok: boolean): void {
        const id = this.heldViewportIds.shift()!;
        this.emit({
          protocol: 3,
          type: "response",
          id,
          ok,
          ...(ok ? {} : { error: { code: "viewport.test", message: "Test rejection" } }),
        });
      }

      private respond(id: string, value?: unknown): void {
        this.emit({
          protocol: 3,
          type: "response",
          id,
          ok: true,
          ...(value === undefined ? {} : { value }),
        });
      }
    }

    const NativeWorker = window.Worker;
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: FakeWorker });
    try {
      const { createFxNode, FxNodeViewDetachedError } = (await import(
        "/src/index.ts" as string
      )) as typeof import("@lib/index.js");
      const api = await createFxNode({ applicationId: "client-test", applicationVersion: 1, resources: {} });
      const worker = FakeWorker.instances[0]!;
      const rootMessages = worker.posted.map((message) => message.type);
      const firstCanvas = document.createElement("canvas"),
        secondCanvas = document.createElement("canvas"),
        viewport = { width: 320, height: 180, dpr: 1 };
      const first = await api.attachView({ canvas: firstCanvas, viewport });
      const second = await api.attachView({
        canvas: secondCanvas,
        viewport,
        initialCamera: { center: { x: 2_000, y: 0 }, zoom: 0.75 },
      });
      const pointerLaneCount = worker.posted.filter(
        (message) =>
          message.type === "view.attach" &&
          typeof SharedArrayBuffer === "function" &&
          message.pointerLane instanceof SharedArrayBuffer,
      ).length;
      let duplicateCode = "";
      try {
        await api.attachView({ canvas: firstCanvas, viewport });
      } catch (error) {
        duplicateCode = (error as { code?: string }).code ?? "";
      }
      worker.emit({
        protocol: 3,
        type: "view.selection.host",
        viewId: first.id,
        projection: {
          nodeCount: 1,
          linkCount: 0,
          canRemove: true,
          mute: { enabled: true, state: "all-unmuted" },
        },
      });
      await new Promise((resolve) => setTimeout(resolve));
      const selections = [first.getHostSnapshot().selection.nodeCount, second.getHostSnapshot().selection.nodeCount];

      const modifiers = { alt: false, control: false, meta: false, shift: false };
      const viewportCount = () => worker.posted.filter((message) => message.type === "view.viewport").length;
      const beforeNoop = viewportCount();
      await first.setViewport(viewport);
      const noopDidNotPost = viewportCount() === beforeNoop;
      worker.holdViewports = true;
      const resizeA = first.setViewport({ width: 321, height: 180, dpr: 1 });
      await new Promise((resolve) => setTimeout(resolve));
      const resizeB = first.setViewport({ width: 322, height: 180, dpr: 1 });
      const resizeC = first.setViewport({ width: 323, height: 180, dpr: 1 });
      const resizeMessagesBeforeA = viewportCount() - beforeNoop;
      const resourceRequests = [0, 0];
      first.onHostRequests((request) => {
        if (request.kind === "resource-open") resourceRequests[0] = resourceRequests[0]! + 1;
      });
      second.onHostRequests((request) => {
        if (request.kind === "resource-open") resourceRequests[1] = resourceRequests[1]! + 1;
      });
      first.feedInput({
        kind: "pointer",
        phase: "down",
        pointerId: 7,
        pointerType: "mouse",
        position: { x: 12, y: 14 },
        button: 0,
        buttons: 1,
        modifiers,
      });
      const inputDuringResize = [...worker.posted]
        .reverse()
        .find((message) => message.type === "view.input" && message.viewId === first.id)!;
      const resizeAWire = worker.posted.find(
        (message) => message.type === "view.viewport" && message.viewId === first.id,
      )!;
      worker.settleViewport(true);
      await resizeA;
      await new Promise((resolve) => setTimeout(resolve));
      const coalescedWire = [...worker.posted]
        .reverse()
        .find((message) => message.type === "view.viewport" && message.viewId === first.id)!;
      worker.settleViewport(false);
      const coalescedSettlements = await Promise.allSettled([resizeB, resizeC]);
      worker.holdViewports = false;
      const resourceOpenRequestId = [...worker.posted]
        .reverse()
        .find((message) => message.type === "view.input" && message.viewId === first.id)
        ?.resourceOpenRequestId as string;
      first.feedInput({
        kind: "pointer",
        phase: "up",
        pointerId: 7,
        pointerType: "mouse",
        position: { x: 12, y: 14 },
        button: 0,
        buttons: 0,
        modifiers,
      });
      worker.emit({
        protocol: 3,
        type: "view.resource.open",
        viewId: first.id,
        requestId: resourceOpenRequestId,
        authorization: { viewId: first.id, token: "node:parameter:image", graphVersion: 0, compositionRevision: 0 },
        resource: {
          id: "image",
          kind: "image",
          title: "Image",
          openTitle: "Open image",
          accept: ["image/png"],
          maxBytes: 1024,
          maxWidth: 64,
          maxHeight: 64,
          maxPixels: 4096,
        },
      });
      await new Promise((resolve) => setTimeout(resolve));
      await first.setViewport({ width: 324, height: 180, dpr: 1 });
      for (const view of [first, second])
        view.feedInput({
          kind: "pointer",
          phase: "move",
          pointerId: 1,
          pointerType: "mouse",
          position: { x: 10, y: 10 },
          button: -1,
          buttons: 0,
          modifiers,
        });
      const beforeDispatch = worker.posted.length;
      await api.dispatch({ type: "undo" });
      const dispatchTrace = worker.posted.slice(beforeDispatch).map((message) => message.type);

      const detachA = first.detach(),
        detachB = first.detach(),
        stableDetachPromise = detachA === detachB;
      let detachedImmediately = false;
      try {
        first.getHostSnapshot();
      } catch (error) {
        detachedImmediately = error instanceof FxNodeViewDetachedError;
      }
      await detachA;
      const replacement = await api.attachView({ canvas: firstCanvas, viewport });
      await replacement.detach();
      await second.detach();
      api.destroy();
      return {
        rootMessages,
        idsDiffer: first.id !== second.id,
        duplicateCode,
        selections,
        resourceRequests,
        dispatchTrace,
        pointerLaneCount,
        noopDidNotPost,
        resizeMessagesBeforeA,
        coalescedWidth: (coalescedWire.viewport as { width: number }).width,
        coalescedSettlements: coalescedSettlements.map((item) => item.status),
        resizeHostGeneration: resizeAWire.hostGeneration,
        inputHostGeneration: inputDuringResize.hostGeneration,
        stableDetachPromise,
        detachedImmediately,
        terminated: worker.terminated,
      };
    } finally {
      Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: NativeWorker });
    }
  });

  expect(result.rootMessages).toEqual(["init"]);
  expect(result.idsDiffer).toBe(true);
  expect(result.duplicateCode).toBe("canvas.in-use");
  expect(result.selections).toEqual([1, 0]);
  // The coalesced resize is posted after the pointer request and intentionally clears that pre-existing request.
  expect(result.resourceRequests).toEqual([0, 0]);
  expect(result.dispatchTrace.at(-1)).toBe("command");
  expect(result.dispatchTrace.filter((type) => type === "view.pointer.flush")).toHaveLength(result.pointerLaneCount);
  expect(result.noopDidNotPost).toBe(true);
  expect(result.resizeMessagesBeforeA).toBe(1);
  expect(result.coalescedWidth).toBe(323);
  expect(result.coalescedSettlements).toEqual(["rejected", "rejected"]);
  expect(result.inputHostGeneration).toBeGreaterThan(result.resizeHostGeneration as number);
  expect(result.stableDetachPromise).toBe(true);
  expect(result.detachedImmediately).toBe(true);
  expect(result.terminated).toBe(true);
});
