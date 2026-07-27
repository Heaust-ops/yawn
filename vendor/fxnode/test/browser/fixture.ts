import { createApplicationFxNode } from "../../examples/blender/application-browser.js";
import { prepareFxNodeBrowserHost } from "../../examples/shared/browser-host.js";
const initialLayout = { graphId: "browser", catalogVersion: 4, nodes: [], links: [], metadata: {} };
window.ready = (async () => {
  const primary = document.querySelector<HTMLCanvasElement>("#primary");
  const addNodeMenuTemplate = document.querySelector<HTMLTemplateElement>("#add-node-menu-template");
  if (!primary || !addNodeMenuTemplate) throw new Error("Primary canvas or add-node menu template missing");
  const host = prepareFxNodeBrowserHost({ canvas: primary, addNodeMenuTemplate });
  let root: Awaited<ReturnType<typeof createApplicationFxNode>> | undefined,
    view: Awaited<ReturnType<NonNullable<typeof root>["attachView"]>> | undefined;
  try {
    root = await createApplicationFxNode();
    await root.setState(initialLayout);
    view = await root.attachView({ canvas: primary, viewport: host.initialViewport });
    host.attach(root, view);
    window.root = root;
    window.view = view;
    window.fxnodeHost = host;
    await view.whenRendered();
    return true;
  } catch (error) {
    host.destroy();
    await view?.detach();
    root?.destroy();
    throw error;
  }
})();
