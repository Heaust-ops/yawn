# Yawn examples

- `render-graph-studio/` is the complete browser integration with FXNode, JSO,
  external pipelines, shared glTF import, mesh handles, and picking.
- `cookbook/` contains small copyable recipes, each focused on one public API.

Start the full browser example with `npm run dev`. The cookbook modules are plain
ES modules and accept a `YawnCore`, mesh, instance, or worker endpoint where a live
renderer is required.
