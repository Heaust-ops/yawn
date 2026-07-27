import { expect, test } from "@playwright/test";
import type { GraphState } from "@lib/core/types.js";

test("browser host disconnect lifecycle is opt-in and preserves same-task reparenting", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  const result = await page.evaluate(async () => {
    const { prepareFxNodeBrowserHost } = await import("../../examples/shared/browser-host.js");
    const makeApi = () => {
      let detaches = 0;
      const api = {
        feedInput() {},
        setViewport() {},
        detach() {
          detaches++;
          return Promise.resolve();
        },
        getHostSnapshot: () => ({ colorPickerOpen: false }),
        onHostRequests: () => () => {},
        onCompositionChanges: () => () => {},
        onMutations: () => () => {},
      } as any;
      return { api, count: () => detaches };
    };
    const explicitCanvas = document.createElement("canvas"),
      automaticCanvas = document.createElement("canvas"),
      replacementParent = document.createElement("div");
    document.body.append(explicitCanvas, automaticCanvas, replacementParent);
    const explicitApi = makeApi(),
      automaticApi = makeApi(),
      explicitHost = prepareFxNodeBrowserHost({ canvas: explicitCanvas }),
      automaticHost = prepareFxNodeBrowserHost({
        canvas: automaticCanvas,
        lifecycle: "detach-on-disconnect",
      });
    explicitHost.attach(explicitApi.api, explicitApi.api);
    automaticHost.attach(automaticApi.api, automaticApi.api);
    explicitCanvas.remove();
    replacementParent.append(automaticCanvas);
    await new Promise((resolve) => setTimeout(resolve));
    const afterReparent = automaticApi.count();
    automaticCanvas.remove();
    await new Promise((resolve) => setTimeout(resolve));
    const resultValue = {
      explicit: explicitApi.count(),
      afterReparent,
      afterRemoval: automaticApi.count(),
    };
    explicitHost.destroy();
    automaticHost.destroy();
    replacementParent.remove();
    return resultValue;
  });
  expect(result).toEqual({ explicit: 0, afterReparent: 0, afterRemoval: 1 });
});

test("core lifecycle owns no DOM state and the host adapter cleans up its policy", async ({ page }) => {
  test.setTimeout(45_000);
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    window.fxnodeHost.destroy();
    window.root.destroy();
    const { createFxNode } = await import("@lib/index.js");
    const { prepareFxNodeBrowserHost } = await import("../../examples/shared/browser-host.js");
    const { APPLICATION_ID, APPLICATION_RESOURCES, APPLICATION_VERSION, APPLICATION_HEADER_STYLES } = await import(
      "../../examples/blender/nodes/application.js"
    );
    const { exampleTheme } = await import("../../examples/shared/theme.js");
    const { anySocket, floatSocket } = await import("../../examples/blender/nodes/socket-types.js");
    const { valueNode } = await import("../../examples/blender/nodes/shader/value.js");
    const { applicationCompatibility } = await import("../../examples/blender/nodes/application.js");
    const layout = { schemaVersion: 1, graphId: "lifecycle", catalogVersion: 7, nodes: [], links: [], metadata: {} };
    const applicationOptions = {
      applicationId: APPLICATION_ID,
      applicationVersion: APPLICATION_VERSION,
      resources: APPLICATION_RESOURCES,
    };
    const state = {
      graphId: layout.graphId as GraphState["graphId"],
      catalogVersion: APPLICATION_VERSION,
      nodes: [],
      links: [],
      metadata: {},
    };
    const canvas = document.querySelector<HTMLCanvasElement>("#primary")!;
    canvas.setAttribute("tabindex", "7");
    canvas.style.touchAction = "pan-x";
    const width = canvas.width,
      height = canvas.height;
    const installValue = async (api: Awaited<ReturnType<typeof createFxNode>>) => {
      await api.setTheme(exampleTheme);
      await api.setHeaderStyles(APPLICATION_HEADER_STYLES);
      await api.composeSocket(...anySocket);
      await api.composeSocket(...floatSocket);
      await api.setCompatibility(applicationCompatibility);
      await api.composeNode(...valueNode);
    };
    let later = 0;
    for (let index = 0; index < 20; index++) {
      const api = await createFxNode(applicationOptions);
      if (index === 0) await installValue(api);
      await api.setState(state, { expectedVersion: 0 });
      if (index === 0) {
        api.onMutations(() => {
          throw new Error("intentional subscriber failure");
        });
        api.onMutations(() => later++);
        await api.dispatch({ type: "node.add", nodeType: "fxnode.shader.value", position: { x: 0, y: 0 } });
      }
      const view = await api.attachView({ canvas, viewport: { width: 1200, height: 640, dpr: 1 } });
      const barrier = view.whenRendered();
      await view.detach();
      api.destroy();
      await barrier.catch((error) => error);
    }
    const host = prepareFxNodeBrowserHost({ canvas }),
      api = await createFxNode(applicationOptions),
      view = await api.attachView({ canvas, viewport: host.initialViewport });
    await api.setState(state, { expectedVersion: 0 });
    host.attach(api, view);
    host.destroy();
    api.destroy();
    return {
      later,
      tabIndex: canvas.getAttribute("tabindex"),
      touchAction: canvas.style.touchAction,
      widthUnchanged: canvas.width === width,
      heightUnchanged: canvas.height === height,
    };
  });
  expect(result).toEqual({
    later: 1,
    tabIndex: "7",
    touchAction: "pan-x",
    widthUnchanged: true,
    heightUnchanged: true,
  });
});

test("bad load retains structured issues and returned values are detached", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    let error: { code?: string; issues?: readonly unknown[] } = {};
    try {
      await window.root.load({ nope: true });
    } catch (value) {
      error = value as typeof error;
    }
    const saved = await window.root.save();
    (saved.nodes as unknown as unknown[]).push({});
    const snapshot = await window.root.getState();
    (snapshot.nodes as unknown as unknown[]).push({});
    const nextSaved = await window.root.save();
    const nextSnapshot = await window.root.getState();
    window.fxnodeHost.destroy();
    window.root.destroy();
    return {
      code: error.code,
      issues: error.issues?.length ?? 0,
      saved: nextSaved.nodes.length,
      snapshot: nextSnapshot.nodes.length,
    };
  });
  expect(result.code).toBeTruthy();
  expect(result.issues).toBeGreaterThan(0);
  expect(result.saved).toBe(0);
  expect(result.snapshot).toBe(0);
});

test("direct client never touches DOM lifecycle APIs", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    window.fxnodeHost.destroy();
    window.root.destroy();
    const { createFxNode } = await import("@lib/index.js"),
      { APPLICATION_ID, APPLICATION_RESOURCES, APPLICATION_VERSION } = await import(
        "../../examples/blender/nodes/application.js"
      ),
      canvas = document.querySelector<HTMLCanvasElement>("#primary")!,
      NativeObserver = ResizeObserver;
    const before = {
      tabindex: canvas.getAttribute("tabindex"),
      touchAction: canvas.style.touchAction,
      width: canvas.width,
      height: canvas.height,
      children: document.body.childElementCount,
    };
    class ThrowingObserver {
      constructor() {
        throw new Error("ResizeObserver constructed");
      }
    }
    Object.defineProperty(window, "ResizeObserver", { configurable: true, writable: true, value: ThrowingObserver });
    const original = {
      add: canvas.addEventListener,
      rect: canvas.getBoundingClientRect,
      focus: canvas.focus,
      capture: canvas.setPointerCapture,
    };
    canvas.addEventListener = (() => {
      throw new Error("listener registered");
    }) as typeof canvas.addEventListener;
    canvas.getBoundingClientRect = (() => {
      throw new Error("layout read");
    }) as typeof canvas.getBoundingClientRect;
    canvas.focus = (() => {
      throw new Error("focused");
    }) as typeof canvas.focus;
    canvas.setPointerCapture = (() => {
      throw new Error("captured");
    }) as typeof canvas.setPointerCapture;
    try {
      const version = APPLICATION_VERSION,
        api = await createFxNode({
          applicationId: APPLICATION_ID,
          applicationVersion: APPLICATION_VERSION,
          resources: APPLICATION_RESOURCES,
        });
      await api.setState(
        {
          graphId: "direct-dom-boundary" as GraphState["graphId"],
          catalogVersion: version,
          nodes: [],
          links: [],
          metadata: {},
        },
        { expectedVersion: 0 },
      );
      api.destroy();
    } finally {
      canvas.addEventListener = original.add;
      canvas.getBoundingClientRect = original.rect;
      canvas.focus = original.focus;
      canvas.setPointerCapture = original.capture;
      Object.defineProperty(window, "ResizeObserver", { configurable: true, writable: true, value: NativeObserver });
    }
    return {
      before,
      after: {
        tabindex: canvas.getAttribute("tabindex"),
        touchAction: canvas.style.touchAction,
        width: canvas.width,
        height: canvas.height,
        children: document.body.childElementCount,
      },
    };
  });
  expect(result.after).toEqual(result.before);
});

test("browser host is exclusive and preserves application DOM changes on cleanup", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    window.fxnodeHost.destroy();
    window.root.destroy();
    const { createFxNode } = await import("@lib/index.js"),
      { prepareFxNodeBrowserHost } = await import("../../examples/shared/browser-host.js"),
      { APPLICATION_ID, APPLICATION_RESOURCES, APPLICATION_VERSION } = await import(
        "../../examples/blender/nodes/application.js"
      ),
      canvas = document.querySelector<HTMLCanvasElement>("#primary")!;
    canvas.removeAttribute("tabindex");
    canvas.style.touchAction = "";
    const host = prepareFxNodeBrowserHost({ canvas });
    let duplicate = "";
    try {
      prepareFxNodeBrowserHost({ canvas });
    } catch (error) {
      duplicate = (error as Error).message;
    }
    const version = APPLICATION_VERSION,
      api = await createFxNode({
        applicationId: APPLICATION_ID,
        applicationVersion: APPLICATION_VERSION,
        resources: APPLICATION_RESOURCES,
      });
    await api.setState(
      {
        graphId: "host-lifecycle" as GraphState["graphId"],
        catalogVersion: version,
        nodes: [],
        links: [],
        metadata: {},
      },
      { expectedVersion: 0 },
    );
    const view = await api.attachView({ canvas, viewport: host.initialViewport });
    host.attach(api, view);
    canvas.setAttribute("tabindex", "9");
    canvas.style.touchAction = "pan-y";
    host.destroy();
    api.destroy();
    const replacement = prepareFxNodeBrowserHost({ canvas });
    replacement.destroy();
    return { duplicate, tabindex: canvas.getAttribute("tabindex"), touchAction: canvas.style.touchAction };
  });
  expect(result).toEqual({
    duplicate: "Canvas already has an active FxNode browser host",
    tabindex: "9",
    touchAction: "pan-y",
  });
});

test("worker resource requests close delayed add-menu UI without intercepting input", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const { prepareFxNodeBrowserHost } = await import("../../examples/shared/browser-host.js"),
      canvas = document.createElement("canvas");
    canvas.style.cssText = "width:200px;height:120px";
    document.body.append(canvas);
    let request: ((value: any) => void) | undefined,
      pickers = 0,
      forwarded = 0;
    const target = Object.freeze({
        kind: "resource-open" as const,
        authorization: Object.freeze({ token: "resource", graphVersion: 0, compositionRevision: 0 }),
        resource: Object.freeze({
          id: "image",
          kind: "image" as const,
          title: "Image",
          openTitle: "Open",
          accept: Object.freeze(["image/png"]),
          maxBytes: 100,
          maxWidth: 10,
          maxHeight: 10,
          maxPixels: 100,
        }),
      }),
      snapshot = Object.freeze({
        compositionRevision: 0,
        colorPickerOpen: false,
        selection: Object.freeze({
          nodeCount: 0,
          linkCount: 0,
          canRemove: false,
          mute: Object.freeze({ enabled: false as const }),
        }),
      });
    const api = {
        feedInput(value: any) {
          if (value.kind === "pointer" && value.phase === "down" && value.button === 0) forwarded++;
        },
        setViewport() {},
        getHostSnapshot: () => snapshot,
        onHostRequests(callback: (value: any) => void) {
          request = callback;
          return () => {};
        },
        onCompositionChanges() {
          return () => {};
        },
        onMutations() {
          return () => {};
        },
        addNode() {
          return Promise.resolve({ status: "noop", version: 0 });
        },
      } as any,
      host = prepareFxNodeBrowserHost({
        canvas,
        activateResourcePicker: () => {
          pickers++;
        },
      });
    host.attach(api, api);
    const rect = canvas.getBoundingClientRect(),
      fire = (pointerId: number, button: number, x: number) => {
        canvas.dispatchEvent(
          new PointerEvent("pointerdown", {
            bubbles: true,
            pointerId,
            pointerType: "mouse",
            button,
            buttons: button === 2 ? 2 : 1,
            clientX: rect.left + x,
            clientY: rect.top + 10,
          }),
        );
      };
    fire(81, 2, 80);
    fire(82, 0, 10);
    request?.(target);
    request?.({
      kind: "add-node-menu",
      viewPosition: { x: 80, y: 10 },
      compositionRevision: 0,
    });
    const menus = document.querySelectorAll("[data-fxnode-add-menu]").length;
    host.destroy();
    canvas.remove();
    return { pickers, menus, forwarded };
  });
  expect(result).toEqual({ pickers: 1, menus: 0, forwarded: 1 });
});

test("default resource picker ignores obsolete asynchronous reads", async ({ page }) => {
  await page.goto("/test/browser/index.html");
  await page.evaluate(() => window.ready);
  const result = await page.evaluate(async () => {
    const { prepareFxNodeBrowserHost } = await import("../../examples/shared/browser-host.js"),
      canvas = document.createElement("canvas");
    canvas.style.cssText = "width:100px;height:100px";
    document.body.append(canvas);
    const target = Object.freeze({
        kind: "resource-open" as const,
        authorization: Object.freeze({ token: "resource", graphVersion: 0, compositionRevision: 0 }),
        resource: Object.freeze({
          id: "image",
          kind: "image" as const,
          title: "Image",
          openTitle: "Open",
          accept: Object.freeze(["image/png"]),
          maxBytes: 100,
          maxWidth: 10,
          maxHeight: 10,
          maxPixels: 100,
        }),
      }),
      snapshot = Object.freeze({
        compositionRevision: 0,
        colorPickerOpen: false,
        selection: Object.freeze({
          nodeCount: 0,
          linkCount: 0,
          canRemove: false,
          mute: Object.freeze({ enabled: false as const }),
        }),
      });
    let resolveA!: (value: ArrayBuffer) => void, resolveB!: (value: ArrayBuffer) => void;
    const a = new File([new Uint8Array([1])], "a.png", { type: "image/png" }),
      b = new File([new Uint8Array([2])], "b.png", { type: "image/png" });
    Object.defineProperty(a, "arrayBuffer", {
      value: () => new Promise<ArrayBuffer>((resolve) => (resolveA = resolve)),
    });
    Object.defineProperty(b, "arrayBuffer", {
      value: () => new Promise<ArrayBuffer>((resolve) => (resolveB = resolve)),
    });
    let request: ((value: any) => void) | undefined;
    const provided: string[] = [],
      api = {
        feedInput() {},
        setViewport() {},
        getHostSnapshot: () => snapshot,
        onHostRequests(callback: (value: any) => void) {
          request = callback;
          return () => {};
        },
        onCompositionChanges() {
          return () => {};
        },
        onMutations() {
          return () => {};
        },
        addNode() {
          return Promise.resolve({ status: "noop", version: 0 });
        },
        provideResource(_authorization: unknown, data: { name: string }) {
          provided.push(data.name);
          return Promise.resolve({ status: "committed", version: provided.length });
        },
      } as any,
      host = prepareFxNodeBrowserHost({ canvas });
    host.attach(api, api);
    const choose = (file: File) => {
      request?.(target);
      const input = document.querySelector<HTMLInputElement>("[data-fxnode-resource-file]")!;
      Object.defineProperty(input, "files", {
        configurable: true,
        value: { 0: file, length: 1, item: (index: number) => (index === 0 ? file : null) },
      });
      input.dispatchEvent(new Event("change"));
    };
    choose(a);
    choose(b);
    resolveB(new Uint8Array([2]).buffer);
    await Promise.resolve();
    await Promise.resolve();
    resolveA(new Uint8Array([1]).buffer);
    await Promise.resolve();
    await Promise.resolve();
    host.destroy();
    canvas.remove();
    return provided;
  });
  expect(result).toEqual(["b.png"]);
});
