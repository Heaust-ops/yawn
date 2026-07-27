# Integration guides

For browser/platform engineers turning an editor bootstrap into a production integration. Start with the [browser host](./browser-host), then wire [interactions](./interactions) and [rendering and lifecycle](./rendering-and-lifecycle). These establish canvas ownership, input forwarding, render checkpoints, and teardown.

Before release, review [browser support](./browser-support), [CSP](./csp), and [accessibility](./accessibility), in that order. The outcome is a host with explicit capability fallbacks, deployable worker policy, accessible DOM-owned controls, and no leaked listeners or workers.
