# Browser host

`createFxNode` needs application identity/version, resource policies, and optionally a worker URL/history limit. It creates the shared root and worker without a canvas. `root.attachView()` needs a canvas, logical CSS-pixel viewport plus DPR, and optionally an initial camera. The **application owns the DOM and canvas dimensions**. Before attachment, measure the initial layout and set the backing dimensions. For a runtime resize, await `view.setViewport(next)` before updating the backing dimensions. Attach and remove your own listeners.

fxnode creates one module worker per root and never creates a resize observer, menu, modal, or file picker. A root may have no views or multiple views. Convert pointer/keyboard/wheel events to each view's `feedInput()` values. The worker performs authoritative hit testing and may issue view-scoped `add-node-menu` or `resource-open` host requests; your DOM decides presentation and ordering.

Use root methods for composition, state, persistence, subscriptions, and context-free commands. Use view methods for input, viewport changes, selection actions, resource responses, and render checkpoints. A canvas can have only one live view. On teardown, remove application listeners and observers first, detach each view, then destroy the root.

The repository example host defaults to `lifecycle: "explicit"`, so `host.destroy()` removes only host-owned policy
and never detaches its view. Opt in with `lifecycle: "detach-on-disconnect"` for component-style examples. That mode
requires an initially connected canvas and `MutationObserver`; hosts share one observer per document. A removal is
confirmed in a microtask (so a same-task remove/reinsert or reparent survives), then host resources are synchronously
removed before `view.detach()` is requested. Moving the canvas to another document counts as disconnection; hiding it
or giving it zero layout size does not.

Resize observations are coalesced while a viewport request is in flight. The host updates canvas backing dimensions
only after `setViewport()` acknowledges that request. A rejection preserves the prior backing store, reports through
`onError`, and a later observation can retry.

Install composition before initial state. Imported/historical data belongs in `load()`, not `setState()`. See [state and persistence](../concepts/state-and-persistence).
