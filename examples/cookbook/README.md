# Yawn addon cookbook

These recipes deliberately avoid another application framework or renderer wrapper.
Import the function you need and pass the same `YawnCore` instance to every addon.

| Recipe | Demonstrates |
| --- | --- |
| `01-canonical-ast.js` | A shared DAG output and canonical S-expression serialization |
| `02-jso-graph.js` | Plain JavaScript object authoring |
| `03-fluent-builder.js` | Fluent render-graph authoring |
| `04-fxnode-export.js` | Exporting an FXNode snapshot to the canonical AST |
| `05-default-pipelines.js` | Attaching optional external WGSL declarations |
| `06-custom-render-pipeline.js` | Supplying a custom scene render shader |
| `07-compute-pipeline.js` | Supplying a binding-free compute pass |
| `08-compile-and-switch.js` | Compiling and transactionally activating a graph |
| `09-gltf-import-worker.js` | Fetching a glTF URL into shared memory from a worker |
| `10-mesh-instances.js` | Creating and mutating conventional instance handles |
| `11-custom-soa-column.js` | Allocating an instance-sized shared SOA column |
| `12-direct-sab-animation.js` | Updating transforms through generation-guarded SAB writes |
| `13-bvh-picking.js` | Ray picking through the mesh-handles addon |
| `14-worker-to-worker.js` | Using core from another worker through a `MessagePort` |
| `15-complete-scene.js` | Combining the graph, glTF, and mesh addons |
| `16-camera-render-data.js` | Treating camera input as direct render-data SAB writes |
| `17-conventional-handles.js` | Conventional camera and material properties over SAB rows |

Recipes 1–7 isolate graph authoring concepts, so their ASTs are intentionally
fragments rather than complete renderable loadouts. Recipe 15 uses the complete
scene graph from `render-graph-studio` when an end-to-end example is needed.
