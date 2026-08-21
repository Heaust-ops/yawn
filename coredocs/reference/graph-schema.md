# Render-graph schema

Field names are case-sensitive camelCase where shown. Unknown JSON fields are ignored by Serde; do not rely on that for versioning. Fields marked required have no default.

## Root and declarations

```text
Graph {
  id: string required, nonempty
  resources: Resources = {}
  pipelines: Pipelines = {}
  passes: Pass[] required, nonempty
}
Resources { buffers: Buffer[] = [], textures: Texture[] = [], samplers: Sampler[] = [] }
Pipelines { render: RenderPipeline[] = [], compute: ComputePipeline[] = [] }
```

Resource IDs must be nonempty and unique across buffers, textures, and samplers. Pipeline IDs must be nonempty and unique across render and compute pipelines. Pass IDs must be nonempty and unique. Only declarations reached from passes survive compilation.

### Buffer

```text
{ id: string required,
  array: string required,          // existing shared-row name at switch time
  usage: string[] = [],
  sync: "frame" | "loadout" = "frame" }
```

Allowed usage names: `uniform`, `storage`, `vertex`, `index`, `indirect`, `copySrc`. `COPY_DST` is always added. Include every way the graph uses the buffer. The allocation is at least 4 bytes, but data copied is the row descriptor's full `bytes`.

### Texture

```text
{ id: string required,
  size: (number | "canvas")[] = [], // width, height, depth/layers
  format: string required,
  usage: string[] = [],
  mipLevelCount: u32 = 1,
  sampleCount: u32 = 1,
  dimension: "1d" | "2d" | "3d" = "2d",
  transient: boolean = true }
```

Missing size components default to canvas width, canvas height, then 1. A string extent is valid only when exactly `"canvas"`. Usage names are `render`, `sampled`, `storage`, `copySrc`, `copyDst`. `format` uses wgpu/WebGPU texture-format spellings such as `rgba8unorm`, `rgba8unorm-srgb`, `bgra8unorm`, `r32float`, `rgba16float`, `depth16unorm`, `depth24plus`, `depth24plus-stencil8`, or `depth32float`; support is device-dependent. `canvas` is allowed only as a fragment target/attachment pseudo-format, not as a declared texture format. Non-transient compatible textures may persist across loadouts; transient textures may alias when lifetimes do not overlap.

### Sampler

```text
{ id: string required, descriptor: object | any = {} }
```

If descriptor is not an object, all defaults apply. Object fields: `addressModeU/V/W` (`clamp-to-edge` default; also `repeat`, `mirror-repeat`), `magFilter`, `minFilter`, `mipmapFilter` (`nearest` default or `linear`), `lodMinClamp` (0), `lodMaxClamp` (32), `compare` (absent or WebGPU compare function: `never`, `less`, `equal`, `less-equal`, `greater`, `not-equal`, `greater-equal`, `always`), and `anisotropyClamp` (1).

## Pipelines

### Render pipeline

```text
{ id: string required, code: WGSL string required,
  vertex: VertexStage = {}, fragment: FragmentStage = {},
  primitive: object | null = null,
  depthStencil: object | null = null,
  multisample: object | null = null }

VertexStage { entry: string = "vertex", buffers: VertexLayout[] = [] }
VertexLayout { arrayStride: u64 required, stepMode: "vertex"|"instance" = "vertex",
               attributes: VertexAttribute[] = [] }
VertexAttribute { format: string required, offset: u64 required, shaderLocation: u32 required }
FragmentStage { entry: string = "fragment", targets: FragmentTarget[] = [] }
FragmentTarget { format: string required, blend: object|null = null, writeMask?: u32 }
```

Vertex formats use wgpu/WebGPU names: `uint8x2/4`, `sint8x2/4`, `unorm8x2/4`, `snorm8x2/4`, `uint16x2/4`, `sint16x2/4`, `unorm16x2/4`, `snorm16x2/4`, `float16x2/4`, `float32`, `float32x2/3/4`, `uint32`, `uint32x2/3/4`, `sint32`, `sint32x2/3/4`, plus formats supported by the current wgpu build.

`primitive` and `blend` deserialize using wgpu's WebGPU-shaped camelCase descriptors. Important allowed primitive values are topology `point-list`, `line-list`, `line-strip`, `triangle-list` (default), `triangle-strip`; strip index format `uint16`/`uint32`; front face `ccw`/`cw`; cull mode absent/`front`/`back`; polygon mode normally `fill`. The `depthStencil` value follows wgpu's Rust Serde field names: it requires `format`, `depth_write_enabled`, and `depth_compare`; optional `stencil` uses `front`, `back`, `read_mask`, and `write_mask`, while optional `bias` uses `constant`, `slope_scale`, and `clamp`. Blend components use factors such as `zero`, `one`, `src`, `one-minus-src`, `src-alpha`, `one-minus-src-alpha`, `dst`, `one-minus-dst` and operations `add`, `subtract`, `reverse-subtract`, `min`, `max`. `writeMask` is ColorWrites bits (`RED=1`, `GREEN=2`, `BLUE=4`, `ALPHA=8`, all=15); omitted means all.

Multisample defaults are `{ count: 1, mask: 2^64-1, alphaToCoverageEnabled: false }`. Fragment targets omitted/empty means no fragment stage. Pipeline target order and formats must match pass color attachments.

### Compute pipeline

```text
{ id: string required, code: WGSL string required, entry: string = "main" }
```

## Passes

```text
Pass {
  id: string required,
  type: "render" | "compute" required,
  pipeline: string required,
  after: string[] = [],
  bindings: Binding[] = [],
  color: ColorAttachment[] = [],
  depth?: DepthAttachment,
  vertexBuffers: VertexBinding[] = [],
  indexBuffer?: IndexBinding,
  draw: Draw = {},
  dispatch: [u32,u32,u32] = [1,1,1]
}
Binding { group: u32 = 0, binding: u32 required, resource: string required,
          offset: u64 = 0, size?: u64 }
ColorAttachment { resource: string required, clear: number[] = [],
                  load: "clear"|"load" = "clear", store: "store"|"discard" = "store" }
DepthAttachment { resource: string required, clear: number = 1,
                  load: "clear"|"load" = "clear", store: "store"|"discard" = "store" }
VertexBinding { slot: u32 = 0, resource: string required, offset: u64 = 0 }
IndexBinding { resource: string required, format: "uint16"|"uint32" = "uint32", offset: u64 = 0 }
Draw { vertices: u32 = 3, indices: u32 = 0, instances: u32 = 1,
       firstVertex: u32 = 0, firstIndex: u32 = 0, baseVertex: i32 = 0,
       firstInstance: u32 = 0 }
```

Render passes use `draw`; when `indexBuffer` exists, the indexed path uses `indices` (so set it nonzero), `firstIndex`, and `baseVertex`. Otherwise it uses `vertices` and `firstVertex`. Both use instance fields. Compute passes use `dispatch`. A pipeline ID is validated against the pass kind when the loadout is built. Binding offset/size and vertex/index offsets are bytes and must satisfy WebGPU alignment and range rules. Attachments require matching usages and formats; sampled/storage bindings likewise require matching texture usage, while WGSL determines binding type and visibility.
