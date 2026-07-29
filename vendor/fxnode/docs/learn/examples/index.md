# Examples

The repository has six current experiences. Images below use the examples' existing captured assets—there are no documentation copies.

## Minimal

![A single Number Value node on the fxnode canvas](../../../examples/assets/minimal.png)

_A minimal composition and one node. [Source](https://github.com/Heaust-ops/fxnode/blob/main/examples/minimal/main.ts)._

## Color Balance

![A Color Balance node with lift, gamma, and gain grading wheels](../../../examples/assets/color-balance.png)

_A focused custom-widget composition. [Source](https://github.com/Heaust-ops/fxnode/blob/main/examples/color-balance/main.ts)._

## Live composition

![A live composition example with a parameter node and upgrade control](../../../examples/assets/live-composition.png)

_Replacing a node definition and migrating its instance. [Source](https://github.com/Heaust-ops/fxnode/blob/main/examples/live-composition/main.ts)._

## Multi-view

![One shared graph rendered in two independent canvas views](../../../examples/assets/multi-view.png)

_One worker and graph with independent cameras and selections. The application-owned toolbar targets the active view, and the canvases forward pointer events only. [Source](https://github.com/Heaust-ops/fxnode/blob/main/examples/multi-view/main.ts)._

## Logic nodes

![Boolean logic nodes connected through vertical multi-input socket pills](../../../examples/assets/logic-nodes.png)

_A Boolean socket, app-composed AND/OR/NOT/XOR/XNOR/NAND/NOR nodes, and five-link inputs. The library presents and edits the graph; the example evaluates it in application code. [Source](https://github.com/Heaust-ops/fxnode/blob/main/examples/logic-nodes/main.ts)._

## Blender-shaped gallery

The [larger gallery source](https://github.com/Heaust-ops/fxnode/blob/main/examples/blender/main.ts) exercises many node and interaction shapes; it is repository application code, not package authority.

These examples present and persist editable graphs; they do **not evaluate** them. Blender-shaped fixtures and visual regression images do not establish Blender compatibility, behavioral parity, or pixel parity.
