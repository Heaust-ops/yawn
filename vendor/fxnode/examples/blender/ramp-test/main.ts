import { createApplicationFxNode, type ApplicationFxNode } from "../application-browser.js";
import type { FxNodeView } from "@lib/index.js";
import { prepareFxNodeBrowserHost } from "../../shared/browser-host.js";
import initialLayout from "./initialLayout.json" with { type: "json" };
const canvas = document.querySelector<HTMLCanvasElement>("#ramp");
if (!canvas) throw new Error("Ramp test canvas missing");
const host = prepareFxNodeBrowserHost({ canvas });
const handle: { root: ApplicationFxNode | null; view: FxNodeView | null; ready: Promise<void> } = {
  root: null,
  view: null,
  ready: Promise.resolve(),
};
(window as typeof window & { rampTest: typeof handle }).rampTest = handle;
handle.ready = (async () => {
  let root: ApplicationFxNode | undefined, view: FxNodeView | undefined;
  try {
    root = await createApplicationFxNode();
    await root.setState(initialLayout);
    view = await root.attachView({ canvas, viewport: host.initialViewport });
    handle.root = root;
    handle.view = view;
    host.attach(root, view);
    await view.whenRendered();
  } catch (error) {
    host.destroy();
    await view?.detach();
    root?.destroy();
    throw error;
  }
})();
