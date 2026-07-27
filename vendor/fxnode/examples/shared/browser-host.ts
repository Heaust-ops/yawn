import type { FxNode, FxNodeModifiers, FxNodeResourceOpenRequest, FxNodeView, FxNodeViewport } from "@lib/index.js";
import { createAddNodeMenu, type AddNodeMenu } from "./add-node-menu.js";

/** Explicit DOM host adapter; callers own attachment and cleanup. */

export interface FxNodeBrowserHostOptions {
  readonly canvas: HTMLCanvasElement;
  readonly lifecycle?: "explicit" | "detach-on-disconnect";
  readonly addNodeMenuTemplate?: HTMLTemplateElement;
  readonly activateResourcePicker?: (request: FxNodeResourceOpenRequest) => void | Promise<void>;
  readonly onError?: (error: unknown) => void;
}

export interface PreparedFxNodeBrowserHost {
  readonly initialViewport: FxNodeViewport;
  readonly currentViewport: FxNodeViewport;
  attach(root: FxNode, view: FxNodeView): void;
  syncViewport(): void;
  destroy(): void;
}

const INPUT_EVENTS = [
  "pointerdown",
  "pointermove",
  "pointerup",
  "pointercancel",
  "mousedown",
  "wheel",
  "keydown",
  "keyup",
  "focus",
  "blur",
] as const;
const activeHosts = new WeakMap<HTMLCanvasElement, PreparedFxNodeBrowserHost>();
type DisconnectRegistration = { canvas: HTMLCanvasElement; disconnected(): void };
const disconnectRegistries = new WeakMap<
  Document,
  { observer: MutationObserver; registrations: Set<DisconnectRegistration> }
>();

function watchDisconnect(documentValue: Document, registration: DisconnectRegistration): () => void {
  let registry = disconnectRegistries.get(documentValue);
  if (!registry) {
    const registrations = new Set<DisconnectRegistration>();
    const Observer = documentValue.defaultView?.MutationObserver;
    if (!Observer) throw new Error("detach-on-disconnect requires a connected canvas and MutationObserver");
    const observer = new Observer(() => {
      for (const candidate of registrations) {
        if (candidate.canvas.ownerDocument === documentValue && candidate.canvas.isConnected) continue;
        queueMicrotask(() => {
          if (
            registrations.has(candidate) &&
            (candidate.canvas.ownerDocument !== documentValue || !candidate.canvas.isConnected)
          )
            candidate.disconnected();
        });
      }
    });
    observer.observe(documentValue, { childList: true, subtree: true });
    registry = { observer, registrations };
    disconnectRegistries.set(documentValue, registry);
  }
  registry.registrations.add(registration);
  return () => {
    registry!.registrations.delete(registration);
    if (registry!.registrations.size === 0) {
      registry!.observer.disconnect();
      disconnectRegistries.delete(documentValue);
    }
  };
}

function measureViewport(canvas: HTMLCanvasElement, devicePixelRatio: number): FxNodeViewport {
  const dpr = Math.min(4, Math.max(1, devicePixelRatio || 1)),
    maxLogicalDimension = Math.floor(8192 / dpr),
    maxLogicalPixels = Math.floor(16_777_216 / (dpr * dpr)),
    width = Math.min(maxLogicalDimension, Math.max(1, canvas.clientWidth)),
    height = Math.min(maxLogicalDimension, Math.floor(maxLogicalPixels / width), Math.max(1, canvas.clientHeight));
  return { width, height, dpr };
}

function sameViewport(left: FxNodeViewport, right: FxNodeViewport): boolean {
  return left.width === right.width && left.height === right.height && left.dpr === right.dpr;
}
function sizeCanvas(canvas: HTMLCanvasElement, viewport: FxNodeViewport): void {
  const width = Math.max(1, Math.round(viewport.width * viewport.dpr)),
    height = Math.max(1, Math.round(viewport.height * viewport.dpr));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
}
function modifiers(event: MouseEvent | KeyboardEvent): FxNodeModifiers {
  return { alt: event.altKey, control: event.ctrlKey, meta: event.metaKey, shift: event.shiftKey };
}
export function prepareFxNodeBrowserHost({
  canvas,
  lifecycle = "explicit",
  addNodeMenuTemplate,
  activateResourcePicker,
  onError = console.error,
}: FxNodeBrowserHostOptions): PreparedFxNodeBrowserHost {
  if (activeHosts.has(canvas)) throw new Error("Canvas already has an active FxNode browser host");
  if (
    lifecycle === "detach-on-disconnect" &&
    (!canvas.isConnected || !canvas.ownerDocument.defaultView?.MutationObserver)
  )
    throw new Error("detach-on-disconnect requires a connected canvas and MutationObserver");
  const ownerDocument = canvas.ownerDocument,
    ownerWindow = ownerDocument.defaultView ?? window,
    report = (error: unknown) => {
      try {
        onError(error);
      } catch (reportError) {
        console.error("FxNode browser host error callback failed", reportError);
      }
    };
  const originalTabIndex = canvas.getAttribute("tabindex"),
    originalTouchAction = canvas.style.touchAction;
  let viewport = measureViewport(canvas, ownerWindow.devicePixelRatio);
  sizeCanvas(canvas, viewport);
  let view: FxNodeView | undefined,
    active = true,
    attached = false,
    lifecycleGeneration = 0,
    authorization: { request: FxNodeResourceOpenRequest; generation: number } | undefined,
    observer: ResizeObserver | undefined;
  let unwatchDisconnect: (() => void) | undefined,
    pendingViewport: FxNodeViewport | undefined,
    resizeInFlight = false;
  let changedTabIndex = false,
    appliedTabIndex: string | null = null,
    changedTouchAction = false,
    pickerGeneration = 0,
    menuPending = false;
  const capturedPointers = new Set<number>(),
    subscriptions = new Set<() => void>();
  let menu: AddNodeMenu | undefined;
  let resourceFile: HTMLInputElement | undefined;

  const defaultPicker = (request: FxNodeResourceOpenRequest) => {
    if (!resourceFile) {
      resourceFile = ownerDocument.createElement("input");
      resourceFile.type = "file";
      resourceFile.hidden = true;
      resourceFile.dataset.fxnodeResourceFile = "";
      ownerDocument.body.append(resourceFile);
      resourceFile.addEventListener("change", resourceChanged);
    }
    const generation = ++pickerGeneration;
    authorization = { request, generation };
    resourceFile.accept = request.resource.accept.join(",");
    resourceFile.value = "";
    resourceFile.click();
  };
  const resourceChanged = () => {
    const pending = authorization,
      file = resourceFile?.files?.[0];
    authorization = undefined;
    if (!pending || !file || !view) return;
    void file
      .arrayBuffer()
      .then((bytes) => {
        if (active && view && pending.generation === pickerGeneration)
          return view.provideResource(pending.request.authorization, { name: file.name, mime: file.type, bytes });
      })
      .catch((error) => {
        if (active && pending.generation === pickerGeneration) report(error);
      });
  };
  const activate = activateResourcePicker ?? defaultPicker;
  const position = (event: MouseEvent) => {
    const rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  const input = (event: Event) => {
    if (!active || !view) return;
    if (event instanceof PointerEvent) {
      const phase =
        event.type === "pointerdown"
          ? "down"
          : event.type === "pointermove"
            ? "move"
            : event.type === "pointerup"
              ? "up"
              : "cancel";
      const point = position(event);
      if (phase === "down") {
        menu?.close(false);
        menuPending = event.button === 2 && !event.ctrlKey && (event.buttons & 1) === 0;
        canvas.focus();
        try {
          canvas.setPointerCapture(event.pointerId);
          capturedPointers.add(event.pointerId);
        } catch {
          /* unsupported or detached */
        }
      }
      if ((phase === "up" || phase === "cancel") && capturedPointers.delete(event.pointerId))
        try {
          if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
        } catch {
          /* detached */
        }
      view.feedInput({
        kind: "pointer",
        phase,
        pointerId: event.pointerId,
        pointerType: event.pointerType,
        position: point,
        button: event.button,
        buttons: event.buttons,
        modifiers: modifiers(event),
      });
      return;
    }
    if (event instanceof WheelEvent) {
      event.preventDefault();
      menu?.close(false);
      menuPending = false;
      const rect = canvas.getBoundingClientRect(),
        scale =
          event.deltaMode === WheelEvent.DOM_DELTA_LINE
            ? 16
            : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
              ? Math.max(1, rect.height)
              : 1;
      view.feedInput({
        kind: "wheel",
        position: { x: event.clientX - rect.left, y: event.clientY - rect.top },
        delta: { x: event.deltaX * scale, y: event.deltaY * scale },
        modifiers: modifiers(event),
      });
      return;
    }
    if (event instanceof MouseEvent) {
      if (event.button !== 2 || (event.buttons & 1) === 0) return;
      menuPending = false;
      view.feedInput({
        kind: "pointer",
        phase: "down",
        pointerId: 1,
        pointerType: "mouse",
        position: position(event),
        button: event.button,
        buttons: event.buttons,
        modifiers: modifiers(event),
      });
      return;
    }
    if (event instanceof KeyboardEvent) {
      menu?.close(false);
      menuPending = false;
      view.feedInput({
        kind: "key",
        phase: event.type === "keydown" ? "down" : "up",
        key: event.key,
        code: event.code,
        repeat: event.repeat,
        modifiers: modifiers(event),
      });
      return;
    }
    view.feedInput({ kind: "focus", phase: event.type === "focus" ? "focus" : "blur" });
  };
  const contextMenu = (event: Event) => event.preventDefault();
  const lostCapture = (event: PointerEvent) => capturedPointers.delete(event.pointerId);
  const outsidePointer = (event: PointerEvent) => {
    if (
      active &&
      view &&
      event.button === 0 &&
      view.getHostSnapshot().colorPickerOpen &&
      event.target !== canvas &&
      !canvas.contains(event.target as Node)
    ) {
      menuPending = false;
      view.feedInput({ kind: "outside-pointer", button: 0 });
    }
  };
  const pumpViewport = () => {
    if (!active || !view || resizeInFlight || !pendingViewport) return;
    const next = pendingViewport,
      generation = lifecycleGeneration;
    pendingViewport = undefined;
    if (sameViewport(viewport, next)) {
      pumpViewport();
      return;
    }
    resizeInFlight = true;
    menu?.close(false);
    menuPending = false;
    void Promise.resolve(view.setViewport(next))
      .then(() => {
        if (!active || generation !== lifecycleGeneration) return;
        viewport = next;
        sizeCanvas(canvas, next);
      })
      .catch((error) => {
        if (active && generation === lifecycleGeneration) report(error);
      })
      .finally(() => {
        if (!active || generation !== lifecycleGeneration) return;
        resizeInFlight = false;
        pumpViewport();
      });
  };
  const syncViewport = () => {
    if (!active) return;
    const next = measureViewport(canvas, ownerWindow.devicePixelRatio);
    if (view) {
      pendingViewport = next;
      pumpViewport();
    } else {
      viewport = next;
      sizeCanvas(canvas, next);
    }
  };
  const resize = () => syncViewport();
  const host: PreparedFxNodeBrowserHost = {
    initialViewport: viewport,
    get currentViewport() {
      return viewport;
    },
    attach(rootValue, viewValue) {
      if (!active) throw new Error("FxNode browser host has been destroyed");
      if (attached) throw new Error("FxNode browser host is already attached");
      if (lifecycle === "detach-on-disconnect" && (!canvas.isConnected || canvas.ownerDocument !== ownerDocument)) {
        host.destroy();
        throw new Error("detach-on-disconnect requires the canvas to remain connected to its original document");
      }
      attached = true;
      view = viewValue;
      try {
        if (addNodeMenuTemplate) menu = createAddNodeMenu(addNodeMenuTemplate, canvas, viewValue, report);
        if (canvas.tabIndex < 0) {
          canvas.tabIndex = 0;
          changedTabIndex = true;
          appliedTabIndex = canvas.getAttribute("tabindex");
        }
        if (canvas.style.touchAction !== "none") {
          canvas.style.touchAction = "none";
          changedTouchAction = true;
        }
        observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(resize);
        for (const name of INPUT_EVENTS) canvas.addEventListener(name, input, { passive: name !== "wheel" });
        canvas.addEventListener("contextmenu", contextMenu);
        canvas.addEventListener("lostpointercapture", lostCapture);
        ownerDocument.addEventListener("pointerdown", outsidePointer, true);
        ownerWindow.addEventListener("resize", resize);
        observer?.observe(canvas);
        if (lifecycle === "detach-on-disconnect")
          unwatchDisconnect = watchDisconnect(ownerDocument, {
            canvas,
            disconnected() {
              const detachedView = view;
              host.destroy();
              void detachedView?.detach().catch(report);
            },
          });
        subscriptions.add(
          viewValue.onHostRequests((request) => {
            if (request.kind === "add-node-menu") {
              if (!menuPending) return;
              menuPending = false;
              menu?.open(request.viewPosition);
              return;
            }
            menuPending = false;
            menu?.close(false);
            try {
              void Promise.resolve(activate(request)).catch((error) => {
                if (active) report(error);
              });
            } catch (error) {
              if (active) report(error);
            }
          }),
        );
        const closeMenu = () => {
          menuPending = false;
          menu?.close(false);
        };
        const invalidatePicker = () => {
          pickerGeneration++;
          authorization = undefined;
          closeMenu();
        };
        subscriptions.add(rootValue.onCompositionChanges(invalidatePicker));
        subscriptions.add(rootValue.onMutations(invalidatePicker));
        syncViewport();
      } catch (error) {
        host.destroy();
        throw error;
      }
    },
    syncViewport,
    destroy() {
      if (!active) return;
      active = false;
      lifecycleGeneration++;
      pendingViewport = undefined;
      unwatchDisconnect?.();
      unwatchDisconnect = undefined;
      for (const unsubscribe of subscriptions) unsubscribe();
      subscriptions.clear();
      pickerGeneration++;
      menuPending = false;
      observer?.disconnect();
      ownerWindow.removeEventListener("resize", resize);
      ownerDocument.removeEventListener("pointerdown", outsidePointer, true);
      for (const name of INPUT_EVENTS) canvas.removeEventListener(name, input);
      canvas.removeEventListener("contextmenu", contextMenu);
      canvas.removeEventListener("lostpointercapture", lostCapture);
      for (const pointerId of capturedPointers)
        try {
          if (canvas.hasPointerCapture(pointerId)) canvas.releasePointerCapture(pointerId);
        } catch {
          /* detached */
        }
      capturedPointers.clear();
      authorization = undefined;
      menu?.destroy();
      if (resourceFile) {
        resourceFile.removeEventListener("change", resourceChanged);
        resourceFile.remove();
        resourceFile = undefined;
      }
      if (changedTouchAction && canvas.style.touchAction === "none") canvas.style.touchAction = originalTouchAction;
      if (changedTabIndex && canvas.getAttribute("tabindex") === appliedTabIndex) {
        if (originalTabIndex === null) canvas.removeAttribute("tabindex");
        else canvas.setAttribute("tabindex", originalTabIndex);
      }
      if (activeHosts.get(canvas) === host) activeHosts.delete(canvas);
      view = undefined;
    },
  };
  activeHosts.set(canvas, host);
  return host;
}
