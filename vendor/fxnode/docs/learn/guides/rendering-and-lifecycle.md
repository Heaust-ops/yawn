# Rendering and lifecycle

Each view's `whenRendered()` synchronizes a frame for that view. Attach another view when a second canvas needs an independent camera or selection over the same graph; graph mutations schedule all attached views. Set the initial backing dimensions before attachment. At runtime, await that view's `setViewport(next)` first, then update the host canvas backing dimensions.

Keep every listener, observer, menu, and focus behavior in an application-owned cleanup object. On unmount/page teardown, remove those resources, await `view.detach()` for each view, then call `root.destroy()`. The example browser host keeps this explicit by default; its opt-in disconnect policy can perform host cleanup and request detachment when a connected canvas is removed. Destroying that host alone never detaches. View detachment is idempotent and rejects subsequent view work with `FxNodeViewDetachedError`. Root destruction is idempotent, detaches all remaining views, and makes pending and future work reject with `FxNodeDestroyedError`. Fatal startup/protocol failures also release resources and make future calls reject the stored terminal error.

Do not use rendering completion as graph execution completion: fxnode never executes graphs.
