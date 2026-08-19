# Package map

Install only the authoring and convenience layers your application needs. None of the addons is required by core's protocol.

<div class="package-grid">
  <a href="./core"><strong>@yawn/core</strong><span>Worker commands, render-graph lifecycle, and shared SOA arrays.</span></a>
  <a href="./render-graph"><strong>@yawn/render-graph-*</strong><span>Canonical AST plus JSO, fluent, and FXNode frontends.</span></a>
  <a href="./gltf-import"><strong>@yawn/gltf-import</strong><span>Worker-side glTF parsing directly into shared upload memory.</span></a>
  <a href="./mesh-handles"><strong>@yawn/mesh-handles</strong><span>Generation-safe mesh, instance, camera, material, and picking facades.</span></a>
  <a href="../recipes/pipelines"><strong>@yawn/default-pipelines</strong><span>Optional scene WGSL and render/compute declarations.</span></a>
  <a href="/playground/"><strong>Examples</strong><span>Editable playgrounds that compose the public packages as an application would.</span></a>
</div>

## Dependency direction

Applications create core first, then pass the same `YawnCore` instance to addons. Addons use public commands and shared descriptors; core never imports an addon.

```text
application ─▶ graph frontend ─▶ graph AST
     │                              │
     ├────▶ glTF / handles addons   │
     │             │                │
     └─────────────┴───────────────▶ core ─▶ render worker
```

This keeps scene policy outside the renderer. You can replace default pipelines, skip conventional handles, or author the AST directly without forking core.
