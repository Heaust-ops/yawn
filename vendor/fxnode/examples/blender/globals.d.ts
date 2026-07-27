import type { FxNode, FxNodeView } from "@lib/index.js";
import type { PreparedFxNodeBrowserHost } from "../shared/browser-host.js";

declare global {
  interface FxNodeExampleHandle {
    root: FxNode | null;
    view: FxNodeView | null;
    host: PreparedFxNodeBrowserHost;
    ready: Promise<void>;
    readonly rendered: Promise<void>;
  }
  interface Window {
    fxnodeExample: FxNodeExampleHandle;
    linkToolsTest: { root: FxNode | null; view: FxNodeView | null; ready: Promise<void> };
  }
}

export {};
