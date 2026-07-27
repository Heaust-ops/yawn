import type { FxNode, FxNodeView } from "@lib/index.js";
import type { PreparedFxNodeBrowserHost } from "../../examples/shared/browser-host.js";

declare global {
  interface Window {
    root: FxNode;
    view: FxNodeView;
    fxnodeHost: PreparedFxNodeBrowserHost;
    ready: Promise<boolean>;
    fxnodeExample: FxNodeExampleHandle;
    parityExample: { root: FxNode; view: FxNodeView };
    controlTest: { root: FxNode | null; view: FxNodeView | null; ready: Promise<void> };
    linkToolsTest: { root: FxNode | null; view: FxNodeView | null; ready: Promise<void> };
    controlEvents: { mutations: number[]; snapshots: number[] };
  }
  interface FxNodeExampleHandle {
    root: FxNode | null;
    view: FxNodeView | null;
    ready: Promise<void>;
    readonly rendered: Promise<void>;
  }
  interface FxNodeEvidenceCounters {
    mutations: number;
    snapshots: number;
  }
}

export {};
