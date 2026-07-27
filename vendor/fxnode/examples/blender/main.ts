import { createApplicationFxNode } from "./application-browser.js";
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";
import initialLayout from "./initialLayout.json" with { type: "json" };

const canvas = document.querySelector<HTMLCanvasElement>("#graph");
const addNodeMenuTemplate = document.querySelector<HTMLTemplateElement>("#add-node-menu-template");
if (!canvas || !addNodeMenuTemplate) throw new Error("Example canvas or add-node menu template is missing");
const host = prepareFxNodeBrowserHost({ canvas, addNodeMenuTemplate });

let resolveRendered!: () => void;
const rendered = new Promise<void>((resolve) => {
  resolveRendered = resolve;
});
const handle: FxNodeExampleHandle = { root: null, view: null, host, ready: Promise.resolve(), rendered };
window.fxnodeExample = handle;

handle.ready = (async () => {
  let root: Awaited<ReturnType<typeof createApplicationFxNode>> | undefined,
    view: Awaited<ReturnType<NonNullable<typeof root>["attachView"]>> | undefined;
  try {
    root = await createApplicationFxNode();
    await root.setState(initialLayout);
    view = await root.attachView({ canvas, viewport: host.initialViewport });
    handle.root = root;
    handle.view = view;
    host.attach(root, view);
    await view.whenRendered();
    resolveRendered();
  } catch (error) {
    host.destroy();
    await view?.detach();
    root?.destroy();
    throw error;
  }
})();
