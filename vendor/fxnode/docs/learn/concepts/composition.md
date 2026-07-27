# Composition

Composition is the application-defined graph language, distinct from graph state. It owns themes, header styles, directional socket compatibility, resources, node definitions, UI rows, defaults, bypasses, and migrations. Graph state is a document written in that language. Compatibility is checked from the destination socket's accepted source types; changing it can invalidate existing links.

There is no built-in registry. Browser applications install definitions through `setTheme`, `setHeaderStyles`, `setCompatibility`, `composeSocket`, and `composeNode`, or atomically with `loadComposition`. Dependencies come first; `setState` comes last. Plain structured-clone-safe data crosses the worker boundary—no callbacks or classes.

Updates compile, validate, rebind, and publish atomically and return a receipt (`status`, composition `revision`, graph version, and whether rebinding changed the graph). A rejected candidate changes nothing. Distinct definition IDs converge regardless of concurrent installation order once dependencies exist; updates to the same ID are ordered, and references still require their dependency to be installed first.

Every committed change to a node definition resets definition-bound undo/redo history, even if no current instance uses that definition. Removing a node definition preserves its instances as opaque, read-only nodes. Removing a socket type is rejected while compatibility rules, another socket type, or a node definition references it; update or remove those dependents first. A valid composition rebind may remove graph links that have become incompatible. Reintroducing compatible definitions can promote opaque instances. A migration `rename-socket` rewrites both the node's socket data and every link endpoint that refers to it in the same transaction—there is no observable half-renamed graph. A semantic no-op emits nothing and advances neither revision nor graph version.

Static/headless authoring can use `compileFxNodeComposition` and immutable helpers to retain literal ID types. Browser handles intentionally accept string IDs because their composition authority can change live.
