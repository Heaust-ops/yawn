import { createFxNode, type FxNodeSocketTypeDefinition, type FxNodeStyleDefinition } from "@lib/index.js";
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";
import { colorBalanceNode } from "../shared/nodes/color-balance.js";
import { exampleTheme } from "../shared/theme.js";
const floatSocket = [
  "float",
  { title: "Float", color: "#a8a8a8", acceptsFrom: ["float"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
const colorSocket = [
  "color",
  { title: "Color", color: "#d7ca63", acceptsFrom: ["color"] },
] as const satisfies readonly [string, FxNodeSocketTypeDefinition];
const styles = { compositorColor: { header: "#8c5cc4" } } as const satisfies Readonly<
  Record<string, FxNodeStyleDefinition>
>;
const canvas = document.querySelector<HTMLCanvasElement>("#graph")!,
  host = prepareFxNodeBrowserHost({ canvas });
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
      applicationId: "fxnode.example.color-balance",
      applicationVersion: 1,
      resources: {},
    });
    if (cleanedUp) {
      root.destroy();
      return;
    }
    handle.root = root;
    await root.setTheme(exampleTheme);
    await root.setHeaderStyles(styles);
    await root.composeSocket(...floatSocket);
    await root.composeSocket(...colorSocket);
    await root.composeNode(...colorBalanceNode);
    await root.setState({ graphId: "color-balance", catalogVersion: 1, nodes: [], links: [], metadata: {} });
    const view = await root.attachView({ canvas, viewport: host.initialViewport });
    handle.view = view;
    host.attach(root, view);
    await view.addNode({ nodeId: "color-balance", typeId: colorBalanceNode[0], viewPosition: { x: 300, y: 40 } });
    await view.whenRendered();
  } catch (error) {
    handle.cleanup();
    throw error;
  }
})();
