import type { FxNodeView } from "@lib/index.js";
import { createApplicationFxNode } from "../blender/application-browser.js";
import initialLayout from "../blender/all-supported/initialLayout.json" with { type: "json" };
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";

const canvases = [
  document.querySelector<HTMLCanvasElement>("#view-a")!,
  document.querySelector<HTMLCanvasElement>("#view-b")!,
];
const toolbar = document.querySelector<HTMLElement>("#toolbar")!;
const nodeType = document.querySelector<HTMLSelectElement>("#node-type")!;
const addButton = toolbar.querySelector<HTMLButtonElement>("[data-action=add]")!;
const deleteButton = toolbar.querySelector<HTMLButtonElement>("[data-action=delete]")!;
const muteButton = toolbar.querySelector<HTMLButtonElement>("[data-action=mute]")!;
const status = toolbar.querySelector<HTMLOutputElement>("#toolbar-status")!;
let activeIndex = 0,
  toolbarPending = false,
  cleaned = false,
  root: Awaited<ReturnType<typeof createApplicationFxNode>> | null = null;
const views: FxNodeView[] = [],
  unsubscribers: (() => void)[] = [];
const hosts = canvases.map((canvas) => prepareFxNodeBrowserHost({ canvas, lifecycle: "detach-on-disconnect" }));
const renderCounts = [0, 0];
function updateToolbar() {
  const snapshot = views[activeIndex]?.getHostSnapshot();
  const muted = snapshot?.selection.mute.enabled === true && snapshot.selection.mute.state === "all-muted";
  addButton.disabled = toolbarPending || !views[activeIndex];
  deleteButton.disabled = toolbarPending || !snapshot?.selection.canRemove;
  muteButton.disabled = toolbarPending || snapshot?.selection.mute.enabled !== true;
  muteButton.setAttribute("aria-pressed", String(muted));
  muteButton.textContent = muted ? "Unmute" : "Mute";
}
function activate(index: number) {
  activeIndex = index;
  canvases.forEach((canvas, candidate) => canvas.parentElement?.classList.toggle("active", candidate === index));
  updateToolbar();
}
const pointerListeners = canvases
  .map((canvas, index) => {
    const listener = () => activate(index);
    canvas.addEventListener("pointerdown", listener);
    return [{ canvas, listener }];
  })
  .flat();
const toolbarListener = async (event: Event) => {
  const action = (event.target as HTMLElement).closest<HTMLButtonElement>("button")?.dataset.action,
    view = views[activeIndex];
  if (!action || !view || toolbarPending) return;
  toolbarPending = true;
  status.textContent = "";
  updateToolbar();
  try {
    const viewport = hosts[activeIndex]!.currentViewport;
    await (action === "add"
      ? view.addNode({
          typeId: nodeType.value,
          viewPosition: { x: viewport.width / 2, y: viewport.height / 2 },
        })
      : action === "delete"
        ? view.removeSelected()
        : view.setSelectedMuted(muteButton.getAttribute("aria-pressed") !== "true"));
  } catch (error) {
    status.textContent = error instanceof Error ? error.message : "Action failed";
  } finally {
    toolbarPending = false;
    if (!cleaned) updateToolbar();
  }
};
toolbar.addEventListener("click", toolbarListener);

async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  window.removeEventListener("pagehide", cleanup);
  toolbar.removeEventListener("click", toolbarListener);
  for (const { canvas, listener } of pointerListeners) canvas.removeEventListener("pointerdown", listener);
  unsubscribers.forEach((unsubscribe) => unsubscribe());
  hosts.forEach((host) => host.destroy());
  await Promise.allSettled(views.map((view) => view.detach()));
  root?.destroy();
  root = null;
  handle.root = null;
  handle.views = [];
}
const handle: MultiViewExampleHandle = { root: null, views, ready: Promise.resolve(), cleanup, renderCounts };
window.fxnodeMultiView = handle;
window.addEventListener("pagehide", cleanup);
handle.ready = (async () => {
  try {
    const created = await createApplicationFxNode();
    if (cleaned) {
      created.destroy();
      return;
    }
    root = created;
    handle.root = created;
    await root.setState(initialLayout);
    for (const [index, canvas] of canvases.entries()) {
      const host = hosts[index]!,
        viewport = host.initialViewport;
      const context = canvas.getContext("2d")!,
        drawImage = context.drawImage.bind(context);
      context.drawImage = ((...args: Parameters<CanvasRenderingContext2D["drawImage"]>) => {
        renderCounts[index]!++;
        Reflect.apply(drawImage, context, args);
      }) as typeof context.drawImage;
      const view = await root.attachView({
        canvas,
        viewport,
        initialCamera: index
          ? { center: { x: 2080, y: -400 }, zoom: 0.45 }
          : { center: { x: 480, y: -550 }, zoom: 0.5 },
      });
      views.push(view);
      host.attach(root, view);
      unsubscribers.push(view.subscribeHost(updateToolbar));
    }
    await Promise.all(views.map((view) => view.whenRendered()));
    updateToolbar();
  } catch (error) {
    await cleanup();
    throw error;
  }
})();
