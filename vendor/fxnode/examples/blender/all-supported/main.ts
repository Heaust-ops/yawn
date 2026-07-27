import { createApplicationFxNode } from "../application-browser.js";
import { prepareFxNodeBrowserHost } from "../../shared/browser-host.js";
import initialLayout from "./initialLayout.json" with { type: "json" };
const canvas = document.querySelector("canvas")!,
  host = prepareFxNodeBrowserHost({ canvas });
let root: Awaited<ReturnType<typeof createApplicationFxNode>> | undefined,
  view: Awaited<ReturnType<NonNullable<typeof root>["attachView"]>> | undefined;
try {
  root = await createApplicationFxNode();
  await root.setState(initialLayout);
  view = await root.attachView({ canvas, viewport: host.initialViewport });
  host.attach(root, view);
  (window as unknown as { fxnodeExample: unknown }).fxnodeExample = { root, view };
  await view.whenRendered();
} catch (error) {
  host.destroy();
  await view?.detach();
  root?.destroy();
  throw error;
}
