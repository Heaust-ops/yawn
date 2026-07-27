import { createFxNode } from "@lib/index.js";
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";
import { exampleTheme } from "../shared/theme.js";
import { minimalStyles, numberSocket, valueNode } from "./definition.js";

const canvas = document.querySelector<HTMLCanvasElement>("#graph")!;
const host = prepareFxNodeBrowserHost({ canvas });
let cleanedUp = false;
function cleanup() {
  window.removeEventListener("pagehide", cleanup);
  cleanedUp = true;
  const root = handle.root,
    view = handle.view;
  handle.root = null;
  handle.view = null;
  host.destroy();
  const destroyRoot = () => root?.destroy();
  if (view) void view.detach().then(destroyRoot, destroyRoot);
  else destroyRoot();
}
const handle: StandaloneExampleHandle = {
  root: null,
  view: null,
  host,
  ready: Promise.resolve(),
  cleanup,
};
window.fxnodeStandalone = handle;
window.addEventListener("pagehide", cleanup);
handle.ready = (async () => {
  try {
    const root = await createFxNode({
      applicationId: "fxnode.example.minimal",
      applicationVersion: 1,
      resources: {},
    });
    if (cleanedUp) {
      root.destroy();
      return;
    }
    handle.root = root;
    await root.setTheme(exampleTheme);
    await root.setHeaderStyles(minimalStyles);
    await root.composeSocket(...numberSocket);
    await root.composeNode(...valueNode);
    await root.setState({ graphId: "minimal", catalogVersion: 1, nodes: [], links: [], metadata: {} });
    const view = await root.attachView({ canvas, viewport: host.initialViewport });
    handle.view = view;
    host.attach(root, view);
    await view.addNode({ nodeId: "value", typeId: valueNode[0], viewPosition: { x: 360, y: 190 } });
    await view.whenRendered();
  } catch (error) {
    handle.cleanup();
    throw error;
  }
})();
