import type { CompositionReceipt, FxNode, FxNodeView } from "@lib/index.js";
import type { PreparedFxNodeBrowserHost } from "./browser-host.js";

declare global {
  interface StandaloneExampleHandle {
    root: FxNode | null;
    view: FxNodeView | null;
    host: PreparedFxNodeBrowserHost;
    ready: Promise<void>;
    graphVersion?: number;
    lastCompositionReceipt?: CompositionReceipt;
    cleanup(): void;
  }
  interface Window {
    fxnodeStandalone: StandaloneExampleHandle;
    fxnodeMultiView: MultiViewExampleHandle;
  }
  interface MultiViewExampleHandle {
    root: FxNode | null;
    views: FxNodeView[];
    ready: Promise<void>;
    cleanup(): Promise<void>;
    renderCounts: number[];
  }
}
export {};
