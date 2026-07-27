<!-- Repository research; excluded from the documentation site. -->

# Blender parity survey

## Current parity matrix

| Area                           | Status                                    | Evidence and limits                                                                                                                                                                                                                         |
| ------------------------------ | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Uniform ordinary controls      | Implemented                               | Number, integer, enum, boolean, string, and vector fields share composition-ordered widget-unit rows. Colors use compact swatches backed by a worker-rendered Oklch/RGBA/HSV/hex picker. Compound widgets intentionally span multiple rows. |
| Field reset                    | Implemented                               | Backspace resets the focused or hovered editable field; linked/read-only fields are inert; reset is one-step undoable.                                                                                                                      |
| Color Ramp                     | Implemented structurally and behaviorally | Browser-tested stop selection/drag, add/remove, modes, interpolation, hue mode, popup color edits, flip, distribute, reset, cancellation, and undo. Eyedropper integration remains unavailable.                                             |
| Noise Texture                  | Implemented structurally                  | All Blender 4.5 dimensions/type visibility combinations are exhaustively tested. fxnode does not evaluate noise.                                                                                                                            |
| Shader Image Texture           | Implemented as editor intent              | Local image selection, worker decoding/thumbnail rendering, projection/interpolation/extension, and Box-only Blend are browser-tested. Texture evaluation remains outside scope.                                                            |
| Compositor Image               | Partial by design                         | Image-user fields are represented and browser-tested. Dynamic multilayer/view/pass sockets require host resource metadata.                                                                                                                  |
| Color Balance                  | Implemented structurally                  | Lift/Gamma/Gain, Offset/Power/Slope, and White Point layouts are browser-tested. “Master Color Grading” is an example label, not a Blender type.                                                                                            |
| Knife and link mute            | Implemented                               | Ctrl-RMB and Ctrl-Alt-RMB freehand gestures are browser-tested as one atomic version/history entry.                                                                                                                                         |
| Node mute                      | Implemented                               | `M` grays every ordinary known node; declared compatible pairs additionally receive derived red bypass curves. There is no graph evaluation.                                                                                                |
| Pixel-exact Blender appearance | Not established                           | Blender captures remain 0/8 pending. Playwright images are fxnode regression and structural-parity evidence only.                                                                                                                           |

## Image Texture and Compositor Image

References: Blender 4.5's [Image Texture manual](https://docs.blender.org/manual/en/4.5/render/shader_nodes/textures/image.html), [Compositor Image manual](https://docs.blender.org/manual/en/4.5/compositing/types/input/image.html), [`ShaderNodeTexImage` RNA](https://docs.blender.org/api/4.5/bpy.types.ShaderNodeTexImage.html), and [`CompositorNodeImage` RNA](https://docs.blender.org/api/4.5/bpy.types.CompositorNodeImage.html).

These are intentionally distinct example composition definitions. Image Texture persists an image reference plus interpolation, projection (including Box-only Blend), extension, editor color-space intent, alpha mode, Vector input, and Color/Alpha outputs. Compositor Image persists its data-block reference and source; Movie/Sequence expose frame count, start, offset, cyclic, and auto-refresh. It has only static Image/Alpha/Z outputs. Resource controls synchronously open a host file chooser, transfer bytes into the worker, decode there, and render a bounded-cache thumbnail. The graph stores only a serializable local reference—not image bytes—so loading that graph in a fresh editor shows the filename/unavailable state until the user reopens the file. Neither node evaluates image pixels. Dynamic multilayer/render passes require a future host metadata provider and are not fabricated.

## Color Balance / “Master Color Grading” example

References: Blender 4.5's [Color Balance manual](https://docs.blender.org/manual/en/4.5/compositing/types/color/adjust/color_balance.html), [`CompositorNodeColorBalance` RNA](https://docs.blender.org/api/4.5/bpy.types.CompositorNodeColorBalance.html), and [commit-pinned Blender compositor node source](https://projects.blender.org/blender/blender/src/commit/8cb6b388974a817afedf1317ce26f0c75aa5f181/source/blender/nodes/composite/nodes/node_composite_colorbalance.cc).

The example definition remains **Color Balance**. “Master Color Grading” is only the parity fixture's custom instance label; Blender has no core Master node. Lift/Gamma/Gain and Offset/Power/Slope use paired scalar/color rows. White Point exposes Input and Output temperature, tint, and color sections. Eyedroppers are visibly disabled placeholders pending a host bridge. This is presentation and persistence only; fxnode performs no compositing evaluation.

## Color Ramp

References: [Blender 4.5 Color Ramp manual](https://docs.blender.org/manual/en/4.5/modeling/geometry_nodes/utilities/color_ramp.html), [Blender 4.5 `ColorRamp` RNA](https://docs.blender.org/api/4.5/bpy.types.ColorRamp.html), and [commit-pinned Blender source](https://projects.blender.org/blender/blender/src/commit/8cb6b388974a817afedf1317ce26f0c75aa5f181/source/blender/editors/interface/templates/interface_template_color_ramp.cc).

The V2 persisted value records RGB/HSV/HSL mode, all five interpolation modes, hue interpolation, and two to 32 sorted, identified RGBA stops. Legacy arrays receive stable index-derived IDs. Worker-owned interactions cover overlapping selection, sampled insertion, Blender-style midpoint insertion, removal, clamped/reordering movement, RGBA updates, flip, even distribution, descriptor reset, cancellation, and one-step undo. The compound row owns toolbar/menu, checker-gradient, handle, selector, position, and color bounds; stops are not sockets. Active-stop selection remains transient and is not serialized.

Deferred Blender details: eyedropper, precise Blender cardinal/B-spline kernels and HSL conversion, context popup styling, and keyboard navigation within popup menus.

## Noise Texture (Blender 4.5)

References: [Blender 4.5 Noise Texture manual](https://docs.blender.org/manual/en/4.5/render/shader_nodes/textures/noise.html) and [Blender 4.5 ShaderNodeTexNoise RNA](https://docs.blender.org/api/4.5/bpy.types.ShaderNodeTexNoise.html).

Dimensions and fractal Type drive V2 `in`/`equals` visibility expressions. Vector is shown for 2D/3D/4D, W for 1D/4D, Normalize only for fBM, Offset for Hybrid/Ridged/Hetero, and Gain for Hybrid/Ridged. Links and defaults remain in the document while their rows are hidden.

## Link knife and mute

`Ctrl`+RMB draws a captured freehand knife and atomically removes every crossed visible effective link. `Ctrl`+`Alt`+RMB uses the same gesture to toggle authored link mute instead; muted and reroute-propagated links are red and do not suppress input defaults. Escape, pointer cancellation, and blur cancel without history. Each completed gesture is one version/event/history entry.

`M` toggles mute for selected ordinary known nodes, including generators. Every muted node receives a clipped neutral overlay. Math, Vector Math, Mix, Set Position, and Transform Geometry additionally show explicitly declared, type-compatible red bypass curves; generator and type-incompatible nodes intentionally have no fabricated bypass. Bypasses are layout-only; fxnode still does not evaluate graphs. Collapse state is immediate and undoable while the header chevron rotates between expanded/down and collapsed/right with a short worker-owned animation.

### Shortcut table

| Shortcut              | Behavior                              |
| --------------------- | ------------------------------------- |
| RMB on empty canvas   | Open searchable add-node dialog       |
| `Ctrl`+RMB drag       | Knife/remove crossed effective links  |
| `Ctrl`+`Alt`+RMB drag | Toggle mute on crossed authored links |
| `M`                   | Toggle selected node mute             |
| `Escape`              | Cancel active gesture silently        |
