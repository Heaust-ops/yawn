# Render graphs from raw JavaScript

## Wire encoder

`compile-graph` does not accept JSON. It accepts one S-expression: `(yawn-graph 1 VALUE)`. Objects are `(object (field KEY VALUE)...)`; arrays are `(array VALUE...)`. Strings use JSON quoting. Bare `true`, `false`, `null`, and JSON numbers become their corresponding values; other bare atoms become strings. Quote all object keys and application strings to avoid ambiguity.

```js
function graphValue(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return `(array ${value.map(graphValue).join(" ")})`;
  if (typeof value === "object") {
    return `(object ${Object.entries(value).map(([key, item]) =>
      `(field ${JSON.stringify(key)} ${graphValue(item)})`
    ).join(" ")})`;
  }
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "boolean") return String(value);
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  throw new TypeError("graph contains an unsupported value");
}
function encodeGraph(graph) {
  return `(yawn-graph 1 ${graphValue(graph)})`;
}
```

Duplicate object fields, malformed/trailing expressions, the wrong tag/version, and empty lists fail with `GRAPH_WIRE`.

## Minimal complete triangle

This graph needs no row arrays. `canvas` is a special attachment and fragment target format; do not declare it as a texture.

```js
const triangle = {
  id: "raw-triangle",
  pipelines: {
    render: [{
      id: "triangle-pipeline",
      code: `
        struct Out { @builtin(position) position: vec4f, @location(0) color: vec3f }
        @vertex fn vertex(@builtin(vertex_index) i: u32) -> Out {
          var positions = array<vec2f, 3>(vec2f(0.0, 0.7), vec2f(-0.7, -0.7), vec2f(0.7, -0.7));
          var colors = array<vec3f, 3>(vec3f(1,0,0), vec3f(0,1,0), vec3f(0,0,1));
          var out: Out; out.position = vec4f(positions[i], 0, 1); out.color = colors[i]; return out;
        }
        @fragment fn fragment(in: Out) -> @location(0) vec4f { return vec4f(in.color, 1); }
      `,
      vertex: { entry: "vertex", buffers: [] },
      fragment: { entry: "fragment", targets: [{ format: "canvas" }] },
    }],
  },
  passes: [{
    id: "triangle",
    type: "render",
    pipeline: "triangle-pipeline",
    color: [{ resource: "canvas", load: "clear", store: "store", clear: [0.02, 0.02, 0.03, 1] }],
    draw: { vertices: 3, instances: 1 },
  }],
};

const id = await request("compile-graph", { serialized: encodeGraph(triangle) });
await request("switch-loadout", { id }); // activates and requests the first frame
```

## Ordering and resources

`passes` declaration order is only a tie-breaker. Each pass's `after` IDs form a dependency DAG; compilation performs a stable-like first-ready topological ordering. Missing dependency → `GRAPH_DEPENDENCY`; cycle → `GRAPH_CYCLE`. Unused resources and pipelines are removed. Used textures receive lifetime slots; compatible transient textures with non-overlapping lifetimes can alias. Therefore do not expect an unused declaration to exist at runtime.

Adjacent compatible render passes may execute in one render pass/bundle. To merge they need matching attachments/sample count, previous `store`, next `load`, matching depth behavior, and the next pass must not bind its attachment as an input. Compute passes remain separate.

Buffers map an entire named SAB row array into a GPU buffer. Bindings are grouped by `group` and use the pipeline's automatically inferred WGSL layout. A binding resource ID may identify a buffer, texture view, or sampler. Vertex/index bindings must identify buffers. Color/depth attachments identify declared textures or special `canvas`. The WGSL declarations, usage flags, offsets, sizes, target formats, and attachment formats must agree; WebGPU validation failures can otherwise surface as generic worker/core errors.

See [Graph schema](/reference/graph-schema) for every field and default.
