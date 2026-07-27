import { createFxNode } from "@lib/index.js";
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";
import { exampleTheme } from "../shared/theme.js";
import { liveNodeV1, liveNodeV2, liveSocket, liveStyles } from "./definitions.js";
const canvas = document.querySelector<HTMLCanvasElement>("#graph")!,
  button = document.querySelector<HTMLButtonElement>("#compose")!,
  status = document.querySelector<HTMLElement>("#status")!,
  host = prepareFxNodeBrowserHost({ canvas });
let cleanedUp = false;
function cleanup() {
  window.removeEventListener("pagehide", cleanup);
  cleanedUp = true;
  button.removeEventListener("click", compose);
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
let revision = 0;
async function compose() {
  if (!handle.root || !handle.view) return;
  button.disabled = true;
  status.textContent = "Composing version 2…";
  try {
    const receipt = await handle.root.composeNode(...liveNodeV2, { expectedRevision: revision });
    revision = receipt.revision;
    handle.lastCompositionReceipt = receipt;
    status.textContent = `Version 2 ${receipt.status}; revision ${receipt.revision}; graph ${receipt.graphChanged ? "migrated" : "unchanged"}`;
    await handle.view.whenRendered();
  } catch (error) {
    status.textContent = error instanceof Error ? error.message : String(error);
    button.disabled = false;
    throw error;
  }
}
button.addEventListener("click", compose);
handle.ready = (async () => {
  try {
    const root = await createFxNode({
      applicationId: "fxnode.example.live-composition",
      applicationVersion: 1,
      resources: {},
    });
    if (cleanedUp) {
      root.destroy();
      return;
    }
    handle.root = root;
    await root.setTheme(exampleTheme);
    await root.setHeaderStyles(liveStyles);
    await root.composeSocket(...liveSocket);
    const receipt = await root.composeNode(...liveNodeV1);
    revision = receipt.revision;
    await root.setState({ graphId: "live", catalogVersion: 1, nodes: [], links: [], metadata: {} });
    const view = await root.attachView({ canvas, viewport: host.initialViewport });
    handle.view = view;
    host.attach(root, view);
    const added = await view.addNode({
      nodeId: "live-node",
      typeId: liveNodeV1[0],
      viewPosition: { x: 340, y: 160 },
    });
    handle.graphVersion = added.version;
    await view.whenRendered();
  } catch (error) {
    handle.cleanup();
    throw error;
  }
})();
