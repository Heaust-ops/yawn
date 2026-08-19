# Yawn examples

- `index.html` is the browser landing page for the complete example and recipes.
- `render-graph-studio/` is the complete browser integration with FXNode, JSO,
  external pipelines, shared glTF import, mesh handles, and picking.
- `cookbook/` contains small copyable recipes, each focused on one public API.

Start the examples index with `npm run examples`. The cookbook modules are plain
ES modules and accept a `YawnCore`, mesh, instance, or worker endpoint where a live
renderer is required.
