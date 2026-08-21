import { YawnCore } from "@yawn/core";
import type {
  ComputePass,
  GraphBuffer,
  GraphSampler,
  GraphTexture,
} from "./ComputePass";

type RowFormat = "f32" | "u32" | "i32";
type MeshLike = {
  id: number;
  geometryId: number;
  indexCount: number;
  vertexCount: number;
  faceMaterials: ReadonlyMap<number, number>;
};
type ShaderLike = {
  id: number;
  code: string;
  vertexEntry: string;
  fragmentEntry: string;
};
type PostProcessState = {
  id: string;
  kind: string;
  options: Record<string, unknown>;
};
type TextureState = GraphTexture & {
  number: number;
  source?: string | ImageBitmap;
};

const rows = [
  ["nodes", 16, "u32"],
  ["nodePositions", 16, "f32"],
  ["nodeRotors", 16, "f32"],
  ["nodeScales", 16, "f32"],
  ["meshInfo", 16, "u32"],
  ["bounds", 32, "f32"],
  ["cameras", 80, "f32"],
  ["cameraMatrices", 80, "f32"],
  ["materials", 48, "f32"],
  ["materialTextures", 32, "u32"],
  ["pointLights", 32, "f32"],
  ["rectAreaLights", 48, "f32"],
  ["spotLights", 32, "f32"],
  ["directionalLights", 32, "f32"],
  ["ambientLights", 16, "f32"],
  ["sceneAccent", 16, "f32"],
] as const;

const clusterShader = /* wgsl */ `
@group(0) @binding(0) var<storage, read> pointLights: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> rectLights: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> spotLights: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> directionalLights: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> ambientLights: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> clusters: array<u32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x != 0u) { return; }
  var count = 0u;
  var light = vec3<f32>(0.0);
  for (var i = 0u; i < arrayLength(&pointLights) / 2u; i++) {
    let enabled = pointLights[i * 2u + 1u].y;
    count += select(0u, 1u, enabled != 0.0);
    light += pointLights[i * 2u].rgb * pointLights[i * 2u].a * enabled * 0.02;
  }
  for (var i = 0u; i < arrayLength(&rectLights) / 3u; i++) {
    let enabled = rectLights[i * 3u + 1u].z;
    count += select(0u, 1u, enabled != 0.0);
    light += rectLights[i * 3u].rgb * rectLights[i * 3u].a * enabled * 0.02;
  }
  for (var i = 0u; i < arrayLength(&spotLights) / 2u; i++) {
    let enabled = spotLights[i * 2u + 1u].w;
    count += select(0u, 1u, enabled != 0.0);
    light += spotLights[i * 2u].rgb * spotLights[i * 2u].a * enabled * 0.02;
  }
  for (var i = 0u; i < arrayLength(&directionalLights) / 2u; i++) {
    let enabled = directionalLights[i * 2u + 1u].x;
    count += select(0u, 1u, enabled != 0.0);
    light += directionalLights[i * 2u].rgb * directionalLights[i * 2u].a * enabled * 0.1;
  }
  for (var i = 0u; i < arrayLength(&ambientLights); i++) {
    count += select(0u, 1u, ambientLights[i].a != 0.0);
    light += ambientLights[i].rgb * ambientLights[i].a;
  }
  clusters[0] = count;
  clusters[1] = bitcast<u32>(light.r);
  clusters[2] = bitcast<u32>(light.g);
  clusters[3] = bitcast<u32>(light.b);
}`;

const basicForwardShader = /* wgsl */ `
struct Accent { color: vec4<f32> }
struct VertexOutput {
  @invariant @builtin(position) position: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) @interpolate(flat) mesh: u32,
  @location(2) @interpolate(flat) material: u32,
}

@group(0) @binding(0) var<storage, read> clusters: array<u32>;
@group(0) @binding(1) var<uniform> accent: Accent;
@group(0) @binding(2) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> rotors: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> scales: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> meshInfo: array<u32>;
@group(0) @binding(6) var<storage, read> materials: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read> cameraMatrices: array<vec4<f32>>;

fn rotate(rotor: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
  return value + 2.0 * cross(rotor.xyz, cross(rotor.xyz, value) + rotor.w * value);
}

@vertex
fn vertex(@location(0) point: vec3<f32>, @builtin(instance_index) packed: u32) -> VertexOutput {
  let instance = packed & 65535u;
  let visible = meshInfo[instance * 4u + 2u];
  let transformed = rotate(rotors[instance], point * scales[instance].xyz) + positions[instance].xyz;
  var clip = vec4<f32>(transformed, 1.0);
  if (cameraMatrices[4].w != 0.0) {
    let world = vec4(transformed, 1.0);
    clip = vec4(
      dot(cameraMatrices[0], world),
      dot(cameraMatrices[1], world),
      dot(cameraMatrices[2], world),
      dot(cameraMatrices[3], world),
    );
  }
  var output: VertexOutput;
  output.position = select(vec4<f32>(2.0, 2.0, 2.0, 1.0), clip, visible != 0u);
  output.normal = normalize(rotate(rotors[instance], vec3<f32>(0.0, 0.0, 1.0)));
  output.mesh = instance;
  output.material = packed >> 16u;
  return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
  let fallback = meshInfo[input.mesh * 4u + 1u];
  let material = select(fallback, input.material - 1u, input.material != 0u);
  let base = materials[material * 3u];
  let properties = materials[material * 3u + 1u];
  let clusterLight = vec3<f32>(bitcast<f32>(clusters[1]), bitcast<f32>(clusters[2]), bitcast<f32>(clusters[3]));
  let light = vec3<f32>(0.12 + max(dot(input.normal, normalize(vec3<f32>(0.4, 0.7, 0.6))), 0.0) * 0.75) + clusterLight;
  let clustered = min(f32(clusters[0]) * 0.002, 0.05);
  let color = base.rgb * (light + clustered) * accent.color.rgb * mix(1.0, 1.1, properties.x);
  return vec4<f32>(color, base.a);
}`;

function pbrShader(mask: number) {
  const baseTexture = mask & 1;
  const materialTexture = mask & 2;
  const normalTexture = mask & 4;
  return /* wgsl */ `
struct VertexOutput {
  @invariant @builtin(position) position: vec4<f32>,
  @location(0) world: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv: vec2<f32>,
  @location(3) tangent: vec4<f32>,
  @location(4) @interpolate(flat) mesh: u32,
  @location(5) @interpolate(flat) material: u32,
}

@group(0) @binding(0) var<storage, read> clusters: array<u32>;
@group(0) @binding(1) var<uniform> accent: vec4<f32>;
@group(0) @binding(2) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> rotors: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> scales: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> meshInfo: array<u32>;
@group(0) @binding(6) var<storage, read> materials: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read> cameraMatrices: array<vec4<f32>>;
${mask ? "@group(1) @binding(0) var materialSampler: sampler;" : ""}
${baseTexture ? "@group(1) @binding(1) var baseTexture: texture_2d<f32>;" : ""}
${materialTexture ? "@group(1) @binding(2) var materialTexture: texture_2d<f32>;" : ""}
${normalTexture ? "@group(1) @binding(3) var normalTexture: texture_2d<f32>;" : ""}

fn rotate(rotor: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
  return value + 2.0 * cross(rotor.xyz, cross(rotor.xyz, value) + rotor.w * value);
}

@vertex
fn vertex(
  @location(0) point: vec3<f32>,
  @location(1) localNormal: vec3<f32>,
  @location(2) uv: vec2<f32>,
  ${normalTexture ? "@location(3) localTangent: vec4<f32>," : ""}
  @builtin(instance_index) packed: u32,
) -> VertexOutput {
  let instance = packed & 65535u;
  let scale = scales[instance].xyz;
  let world = rotate(rotors[instance], point * scale) + positions[instance].xyz;
  var clip = vec4<f32>(world, 1.0);
  if (cameraMatrices[4].w != 0.0) {
    let homogeneous = vec4(world, 1.0);
    clip = vec4(
      dot(cameraMatrices[0], homogeneous),
      dot(cameraMatrices[1], homogeneous),
      dot(cameraMatrices[2], homogeneous),
      dot(cameraMatrices[3], homogeneous),
    );
  }
  var output: VertexOutput;
  output.position = select(vec4<f32>(2.0, 2.0, 2.0, 1.0), clip, meshInfo[instance * 4u + 2u] != 0u);
  output.world = world;
  output.normal = normalize(rotate(rotors[instance], localNormal / scale));
  output.uv = uv;
  output.tangent = ${normalTexture ? "vec4(normalize(rotate(rotors[instance], localTangent.xyz * scale)), localTangent.w)" : "vec4(1.0, 0.0, 0.0, 1.0)"};
  output.mesh = instance;
  output.material = packed >> 16u;
  return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
  let fallback = meshInfo[input.mesh * 4u + 1u];
  let material = select(fallback, input.material - 1u, input.material != 0u);
  let factor = materials[material * 3u];
  let properties = materials[material * 3u + 1u];
  let extra = materials[material * 3u + 2u];
  let base = factor * ${baseTexture ? "textureSample(baseTexture, materialSampler, input.uv)" : "vec4(1.0)"};
  let packedMaterial = ${materialTexture ? "textureSample(materialTexture, materialSampler, input.uv)" : "vec4(1.0)"};
  let metallic = clamp(properties.x * ${materialTexture ? "packedMaterial.b" : "1.0"}, 0.0, 1.0);
  let roughness = clamp(properties.y * ${materialTexture ? "packedMaterial.g" : "1.0"}, 0.04, 1.0);
  var normal = normalize(input.normal);
  ${normalTexture ? "let tangent = normalize(input.tangent.xyz - normal * dot(input.tangent.xyz, normal)); let bitangent = cross(normal, tangent) * input.tangent.w; let mapped = textureSample(normalTexture, materialSampler, input.uv).xyz * 2.0 - 1.0; normal = normalize(mat3x3<f32>(tangent, bitangent, normal) * vec3(mapped.xy * extra.y, mapped.z));" : ""}
  let view = normalize(cameraMatrices[4].xyz - input.world);
  let lightDirection = normalize(vec3<f32>(0.4, 0.7, 0.6));
  let halfVector = normalize(lightDirection + view);
  let nDotL = max(dot(normal, lightDirection), 0.0);
  let nDotH = max(dot(normal, halfVector), 0.0);
  let f0 = mix(vec3(0.04), base.rgb, metallic);
  let specular = f0 * pow(nDotH, max(2.0, 2.0 / (roughness * roughness) - 2.0));
  let clusterLight = vec3<f32>(bitcast<f32>(clusters[1]), bitcast<f32>(clusters[2]), bitcast<f32>(clusters[3]));
  let ambient = vec3(0.08) + clusterLight;
  let diffuse = base.rgb * (1.0 - metallic) * nDotL;
  let emissive = vec3(properties.z, properties.w, extra.x);
  return vec4((base.rgb * ambient + diffuse + specular * nDotL + emissive) * accent.a, base.a);
}`;
}

const emptyForwardShader = /* wgsl */ `
struct Accent { color: vec4<f32> }
@group(0) @binding(0) var<uniform> accent: Accent;
struct VertexOutput { @builtin(position) position: vec4<f32> }
@vertex fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
  let points = array(vec2(-0.72, -0.6), vec2(0.72, -0.6), vec2(0.0, 0.72));
  var output: VertexOutput;
  output.position = vec4(points[index], 0.0, 1.0);
  return output;
}
@fragment fn fragment() -> @location(0) vec4<f32> {
  return vec4(accent.color.rgb, 1.0);
}`;

const fullscreenVertex = /* wgsl */ `
struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
}
@vertex fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
  let points = array(vec2(-1.0, -3.0), vec2(3.0, 1.0), vec2(-1.0, 1.0));
  var output: VertexOutput;
  output.position = vec4(points[index], 0.0, 1.0);
  output.uv = points[index] * vec2(0.5, -0.5) + vec2(0.5);
  return output;
}`;

function effectFragment(kind: string, options: Record<string, unknown>) {
  const amount = Number(options.amount ?? options.exposure ?? 1);
  const safeAmount = Number.isFinite(amount) ? amount : 1;
  const body: Record<string, string> = {
    ssao: `let value = textureSample(source, sourceSampler, input.uv); return vec4(value.rgb * ${Math.max(0, 1 - safeAmount * 0.2)}, value.a);`,
    fxaa: `let size = vec2<f32>(textureDimensions(source)); let pixel = 1.0 / size; let center = textureSample(source, sourceSampler, input.uv); let around = textureSample(source, sourceSampler, input.uv + vec2(pixel.x, 0.0)) + textureSample(source, sourceSampler, input.uv - vec2(pixel.x, 0.0)) + textureSample(source, sourceSampler, input.uv + vec2(0.0, pixel.y)) + textureSample(source, sourceSampler, input.uv - vec2(0.0, pixel.y)); return mix(center, around * 0.25, 0.35);`,
    colorGrading: `let value = textureSample(source, sourceSampler, input.uv); return vec4(pow(max(value.rgb * ${safeAmount}, vec3(0.0)), vec3(1.0 / 2.2)), value.a);`,
    dynamicExposure: `let value = textureSample(source, sourceSampler, input.uv); return vec4(value.rgb * ${safeAmount}, value.a);`,
    silhouette: `let size = vec2<f32>(textureDimensions(source)); let pixel = 1.0 / size; let value = textureSample(source, sourceSampler, input.uv); let edge = length(value.rgb - textureSample(source, sourceSampler, input.uv + pixel).rgb); return vec4(mix(value.rgb, vec3(0.0), smoothstep(0.08, 0.2, edge)), value.a);`,
    edges: `let size = vec2<f32>(textureDimensions(source)); let pixel = 1.0 / size; let value = textureSample(source, sourceSampler, input.uv); let dx = length(value.rgb - textureSample(source, sourceSampler, input.uv + vec2(pixel.x, 0.0)).rgb); let dy = length(value.rgb - textureSample(source, sourceSampler, input.uv + vec2(0.0, pixel.y)).rgb); return vec4(vec3(max(dx, dy) * ${safeAmount}), value.a);`,
  };
  return `${fullscreenVertex}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var sourceSampler: sampler;
@fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
  ${body[kind] ?? "return textureSample(source, sourceSampler, input.uv);"}
}`;
}

function presentShader(toneMap: string) {
  const tone =
    toneMap === "reinhard"
      ? "color / (color + vec3(1.0))"
      : toneMap === "linear"
        ? "clamp(color, vec3(0.0), vec3(1.0))"
        : "clamp((color * (2.51 * color + vec3(0.03))) / (color * (2.43 * color + vec3(0.59)) + vec3(0.14)), vec3(0.0), vec3(1.0))";
  return `${fullscreenVertex}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var sourceSampler: sampler;
@fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
  let value = textureSample(source, sourceSampler, input.uv);
  let color = max(value.rgb, vec3(0.0));
  return vec4(${tone}, value.a);
}`;
}

function encode(value: unknown): string {
  if (value === null || typeof value === "boolean" || typeof value === "number")
    return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value))
    return `(array${value.map((item) => ` ${encode(item)}`).join("")})`;
  if (value && value.constructor === Object)
    return `(object${Object.keys(value as object)
      .sort()
      .map(
        (key) =>
          ` (field ${JSON.stringify(key)} ${encode((value as Record<string, unknown>)[key])})`,
      )
      .join("")})`;
  throw new TypeError("Render graph values must be plain data");
}

function serialize(graph: object) {
  return `(yawn-graph 1 ${encode(graph)})`;
}

const typedArrayMutators = new Set([
  "copyWithin",
  "fill",
  "reverse",
  "set",
  "sort",
]);

/** The conventional single-loadout scene layer; hot values always remain direct SAB writes. */
export class Scene {
  readonly core: YawnCore;
  readonly ready: Promise<this>;
  readonly hdr: boolean;
  #graphUpdates = Promise.resolve();
  #computePasses = new Map<string, ComputePass>();
  #meshes = new Map<number, MeshLike>();
  #shaders = new Map<number, ShaderLike>();
  #effects = new Map<string, PostProcessState>();
  #textures = new Map<number, TextureState>();
  #geometry = new Map<number, Map<string, Float32Array | Uint32Array>>();
  #geometryRefs = new Map<number, number>();
  #nextGeometry = 1;
  #nextTexture = 0;
  #graphBatchDepth = 0;
  #graphBatchDirty = false;
  #writeBatchDepth = 0;
  #writeBatchDirty = false;
  #writeBatchBundleDirty = false;
  #signals?: Float32Array;
  #arrays = new WeakMap<object, object>();
  #views = new WeakMap<object, object>();

  constructor(
    canvas: HTMLCanvasElement,
    options: { arenaBytes?: number; debug?: boolean; fps?: number; hdr?: boolean } = {},
  ) {
    this.hdr = options.hdr ?? true;
    this.core = new YawnCore(canvas, {
      arenaBytes: options.arenaBytes,
      debug: options.debug,
    });
    this.ready = this.#initialize(options.fps);
  }

  async #initialize(fps?: number) {
    await this.core.ready;
    this.#signals = this.core.array("signals").row(0);
    for (const [name, stride, format] of rows)
      await this.core.createRows({ name, rows: 1, stride, format });
    await this.core.createRows({
      name: "clusters",
      rows: 256,
      stride: 16,
      format: "u32",
    });
    this.core.array("nodeRotors").write(0, [0, 0, 0, 1]);
    this.core.array("nodeScales").write(0, [1, 1, 1, 0]);
    this.core.array("sceneAccent").write(0, [0.28, 0.72, 1, 1]);
    const material = await this.core.allocateObject("materials");
    this.core
      .array("materials")
      .write(material, [1, 1, 1, 1, 0, 0.7, 0, 0, 0, 0, 1, 0.5]);
    if (fps !== undefined) await this.core.setFps(fps);
    await this.#compileRenderGraph();
    return this;
  }

  array(name: string) {
    const array = this.core.array(name);
    const current = this.#arrays.get(array);
    if (current) return current as typeof array;
    let proxy: typeof array;
    proxy = new Proxy(array, {
      get: (target, property) => {
        if (property === "row")
          return (index: number) => this.#mutable(target.row(index));
        if (property === "view") return this.#mutable(target.view);
        if (property === "write")
          return (index: number, values: ArrayLike<number>) => {
            target.write(index, values);
            this.markDirty();
            return proxy;
          };
        const value = Reflect.get(target, property, target);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    this.#arrays.set(array, proxy);
    return proxy;
  }

  #mutable<T extends Float32Array | Uint32Array | Int32Array>(view: T): T {
    const current = this.#views.get(view);
    if (current) return current as T;
    let proxy: T;
    proxy = new Proxy(view, {
      get: (target, property) => {
        if (property === "subarray")
          return (begin?: number, end?: number) =>
            this.#mutable(target.subarray(begin, end) as T);
        const value = Reflect.get(target, property, target);
        if (typeof value !== "function") return value;
        if (property === "constructor") return value;
        if (!typedArrayMutators.has(String(property))) return value.bind(target);
        return (...arguments_: unknown[]) => {
          const result = Reflect.apply(value, target, arguments_);
          this.markDirty();
          return result === target ? proxy : result;
        };
      },
      set: (target, property, value) => {
        const written = Reflect.set(target, property, value, target);
        if (written) this.markDirty();
        return written;
      },
    });
    this.#views.set(view, proxy);
    return proxy;
  }

  markDirty(bundle = false) {
    if (this.#writeBatchDepth) {
      this.#writeBatchDirty = true;
      this.#writeBatchBundleDirty ||= bundle;
      return;
    }
    if (!this.#signals) return;
    this.#signals[5] = 1;
    if (bundle) this.#signals[6] = 1;
  }

  /** Defers the dirty signal until a synchronous group of SAB writes is complete. */
  batchWrites<T>(operation: () => T) {
    this.#writeBatchDepth++;
    try {
      return operation();
    } finally {
      this.#writeBatchDepth--;
      if (!this.#writeBatchDepth && this.#writeBatchDirty) {
        const bundle = this.#writeBatchBundleDirty;
        this.#writeBatchDirty = false;
        this.#writeBatchBundleDirty = false;
        this.markDirty(bundle);
      }
    }
  }

  async ensureRows(
    name: string,
    rowCount: number,
    stride: number,
    format: RowFormat,
  ) {
    await this.core.ready;
    try {
      const current = this.core.array(name);
      if (current.stride !== stride || current.format !== format)
        throw new Error(`ROW_LAYOUT: ${name}`);
      if (current.rows >= rowCount) return current;
    } catch (error) {
      if (
        !(error instanceof Error) ||
        !error.message.startsWith("UNKNOWN_ARRAY")
      )
        throw error;
    }
    return this.core.createRows({
      name,
      rows: Math.max(1, rowCount),
      stride,
      format,
    });
  }

  async reserve(additional: { nodes?: number; materials?: number }) {
    await this.ready;
    const nodes = additional.nodes ?? 0;
    const materials = additional.materials ?? 0;
    if (
      ![nodes, materials].every(
        (value) => Number.isInteger(value) && value >= 0,
      )
    )
      throw new RangeError("additional");
    const nodeCapacity = this.array("nodes").rows + nodes;
    const materialCapacity = this.array("materials").rows + materials;
    const growth = [
      ...rows.slice(0, 6).map(([name, stride, format]) => ({
        name,
        rows: nodeCapacity,
        stride,
        format,
      })),
      {
        name: "materials",
        rows: materialCapacity,
        stride: 48,
        format: "f32" as const,
      },
      {
        name: "materialTextures",
        rows: materialCapacity,
        stride: 32,
        format: "u32" as const,
      },
    ].filter((request) => this.array(request.name).rows < request.rows);
    if (growth.length) await this.core.createRowsBatch(growth);
  }

  async batchGraphUpdates<T>(operation: () => T | Promise<T>) {
    await this.ready;
    this.#graphBatchDepth++;
    try {
      return await operation();
    } finally {
      this.#graphBatchDepth--;
      if (!this.#graphBatchDepth && this.#graphBatchDirty) {
        this.#graphBatchDirty = false;
        await this.updateRenderGraph();
      }
    }
  }

  async allocateNode() {
    await this.ready;
    const id = await this.core.allocateObject("nodes");
    const growth = rows.slice(1, 6).flatMap(([name, stride, format]) => {
      const current = this.array(name);
      return current.rows < id + 1
        ? [{ name, rows: id + 1, stride, format }]
        : [];
    });
    if (growth.length) await this.core.createRowsBatch(growth);
    this.array("nodeRotors").write(id, [0, 0, 0, 1]);
    this.array("nodeScales").write(id, [1, 1, 1, 0]);
    this.array("nodes").write(id, [1, 0, 0, 0]);
    return id;
  }

  async releaseNode(id: number) {
    for (const name of [
      "nodes",
      "nodePositions",
      "nodeRotors",
      "nodeScales",
      "meshInfo",
      "bounds",
    ])
      this.array(name).row(id).fill(0);
    await this.core.deleteObject("nodes", id);
  }

  async allocateMaterial() {
    await this.ready;
    const id = await this.core.allocateObject("materials");
    await this.ensureRows("materialTextures", id + 1, 32, "u32");
    return id;
  }

  addComputePass(pass: ComputePass) {
    if (this.#computePasses.has(pass.id))
      throw new Error(`COMPUTE_PASS_EXISTS: ${pass.id}`);
    this.#computePasses.set(pass.id, pass);
    pass.attach(this);
    return this.updateRenderGraph();
  }

  removeComputePass(pass: ComputePass | string) {
    const id = typeof pass === "string" ? pass : pass.id;
    const existing = this.#computePasses.get(id);
    existing?.attach(undefined);
    this.#computePasses.delete(id);
    return this.updateRenderGraph();
  }

  registerMesh(mesh: MeshLike) {
    this.#meshes.set(mesh.id, mesh);
    this.#geometryRefs.set(
      mesh.geometryId,
      (this.#geometryRefs.get(mesh.geometryId) ?? 0) + 1,
    );
    return this.updateRenderGraph();
  }

  async unregisterMesh(mesh: MeshLike) {
    this.#meshes.delete(mesh.id);
    const references = Math.max(
      0,
      (this.#geometryRefs.get(mesh.geometryId) ?? 1) - 1,
    );
    this.#geometryRefs.set(mesh.geometryId, references);
    await this.updateRenderGraph();
    if (!references) {
      for (const kind of this.#geometry.get(mesh.geometryId)?.keys() ?? [])
        await this.core.deleteRows(`geometry.${mesh.geometryId}.${kind}`);
      this.#geometry.delete(mesh.geometryId);
      this.#geometryRefs.delete(mesh.geometryId);
    }
  }

  createGeometry() {
    const id = this.#nextGeometry++;
    this.#geometry.set(id, new Map());
    this.#geometryRefs.set(id, 0);
    return id;
  }

  geometryReferences(id: number) {
    return this.#geometryRefs.get(id) ?? 0;
  }

  referenceGeometry(id: number) {
    this.#geometryRefs.set(id, (this.#geometryRefs.get(id) ?? 0) + 1);
  }

  releaseGeometry(id: number) {
    this.#geometryRefs.set(
      id,
      Math.max(0, (this.#geometryRefs.get(id) ?? 1) - 1),
    );
  }

  async cloneGeometry(id: number) {
    const clone = this.createGeometry();
    for (const [kind, data] of this.#geometry.get(id) ?? [])
      await this.setVertexData(
        clone,
        kind,
        data.slice() as Float32Array | Uint32Array,
        false,
      );
    return clone;
  }

  async setVertexData(
    geometry: number,
    kind: string,
    source: ArrayLike<number>,
    updateGraph = true,
  ) {
    const components: Record<string, number> = {
      positions: 3,
      normals: 3,
      tangents: 4,
      uvs: 2,
      colors: 4,
      indices: 1,
    };
    const width = components[kind];
    if (!width || source.length % width)
      throw new RangeError(`VERTEX_DATA: ${kind}`);
    const integer = kind === "indices";
    const data = integer ? Uint32Array.from(source) : Float32Array.from(source);
    const name = `geometry.${geometry}.${kind}`;
    const rowCount = integer ? Math.ceil(data.length / 4) : data.length / width;
    const target = await this.ensureRows(
      name,
      rowCount,
      16,
      integer ? "u32" : "f32",
    );
    if (updateGraph) this.markDirty(true);
    const view = target.view;
    view.fill(0);
    if (integer || width === 4) view.set(data);
    else
      for (let row = 0; row < rowCount; row++)
        for (let lane = 0; lane < width; lane++)
          view[row * 4 + lane] = data[row * width + lane];
    (
      this.#geometry.get(geometry) ??
      this.#geometry.set(geometry, new Map()).get(geometry)!
    ).set(kind, data);
    if (updateGraph) await this.updateRenderGraph();
  }

  geometryData(id: number, kind: string) {
    return this.#geometry.get(id)?.get(kind);
  }

  registerShader(material: ShaderLike) {
    this.#shaders.set(material.id, material);
    return this.updateRenderGraph();
  }

  unregisterShader(id: number) {
    this.#shaders.delete(id);
    return this.updateRenderGraph();
  }

  registerTexture(texture: Omit<TextureState, "number">) {
    const number = this.#nextTexture++;
    this.#textures.set(number, { ...texture, number });
    return { number, ready: this.updateRenderGraph() };
  }

  unregisterTexture(number: number) {
    this.#textures.delete(number);
    return this.updateRenderGraph();
  }

  setPostProcess(effect: PostProcessState, enabled: boolean) {
    if (enabled) this.#effects.set(effect.id, effect);
    else this.#effects.delete(effect.id);
    return this.updateRenderGraph();
  }

  updateRenderGraph() {
    this.markDirty(true);
    if (this.#graphBatchDepth) {
      this.#graphBatchDirty = true;
      return Promise.resolve();
    }
    const update = this.#graphUpdates.then(async () => {
      await this.ready;
      await this.#compileRenderGraph();
    });
    this.#graphUpdates = update.catch(() => undefined);
    return update;
  }

  async #compileRenderGraph() {
    const buffers = new Map<string, GraphBuffer>();
    const textures = new Map<string, GraphTexture>();
    const samplers = new Map<string, GraphSampler>();
    const computePipelines: object[] = [];
    const renderPipelines: object[] = [];
    const passes: object[] = [];
    const addBuffer = (value: GraphBuffer) => buffers.set(value.id, value);
    const addTexture = (value: GraphTexture) => textures.set(value.id, value);
    const addSampler = (value: GraphSampler) => samplers.set(value.id, value);

    for (const [id, array] of [
      ["point-lights", "pointLights"],
      ["rect-lights", "rectAreaLights"],
      ["spot-lights", "spotLights"],
      ["directional-lights", "directionalLights"],
      ["ambient-lights", "ambientLights"],
      ["clusters", "clusters"],
    ])
      addBuffer({ id, array, usage: ["storage"] });
    addBuffer({ id: "accent", array: "sceneAccent", usage: ["uniform"] });
    computePipelines.push({
      id: "cluster-lights",
      code: clusterShader,
      entry: "main",
    });
    passes.push({
      id: "cluster-lights",
      type: "compute",
      pipeline: "cluster-lights",
      dispatch: [4, 1, 1],
      bindings: [
        "point-lights",
        "rect-lights",
        "spot-lights",
        "directional-lights",
        "ambient-lights",
        "clusters",
      ].map((resource, binding) => ({ group: 0, binding, resource })),
    });

    for (const pass of this.#computePasses.values()) {
      pass.buffers.forEach(addBuffer);
      pass.textures.forEach(addTexture);
      pass.samplers.forEach(addSampler);
      computePipelines.push({
        id: pass.id,
        code: pass.code,
        entry: pass.entry,
      });
      passes.push({
        id: pass.id,
        type: "compute",
        pipeline: pass.id,
        after: pass.after.length ? pass.after : ["cluster-lights"],
        bindings: pass.bindings,
        dispatch: pass.dispatch,
      });
    }

    const computeIds = [...this.#computePasses.keys()];
    const renderedMeshes = [...this.#meshes.values()].filter(
      (mesh) => mesh.vertexCount > 0,
    );
    const hdrFormat = this.hdr ? "rgba16float" : "rgba8unorm";
    addTexture({
      id: "hdr",
      format: hdrFormat,
      size: ["canvas", "canvas", 1],
      usage: ["render", "sampled"],
      transient: false,
    });
    addTexture({
      id: "depth",
      format: "depth24plus",
      size: ["canvas", "canvas", 1],
      usage: ["render"],
      transient: true,
    });
    addSampler({ id: "linear", magFilter: "linear", minFilter: "linear" });
    addSampler({
      id: "material-linear",
      magFilter: "linear",
      minFilter: "linear",
      mipmapFilter: "linear",
      addressModeU: "repeat",
      addressModeV: "repeat",
      anisotropyClamp: 16,
    });
    for (const { number: _, source: __, ...texture } of this.#textures.values())
      addTexture(texture);

    let previous = computeIds.length ? computeIds : ["cluster-lights"];
    if (!renderedMeshes.length) {
      renderPipelines.push({
        id: "empty-forward",
        code: emptyForwardShader,
        vertex: { entry: "vertex" },
        fragment: { entry: "fragment", targets: [{ format: hdrFormat }] },
      });
      passes.push({
        id: "forward-empty",
        type: "render",
        pipeline: "empty-forward",
        after: previous,
        bindings: [{ group: 0, binding: 0, resource: "accent" }],
        color: [{ resource: "hdr", clear: [0.015, 0.025, 0.05, 1] }],
        draw: { vertices: 3 },
      });
      previous = ["forward-empty"];
    } else {
      for (const [id, array] of [
        ["node-positions", "nodePositions"],
        ["node-rotors", "nodeRotors"],
        ["node-scales", "nodeScales"],
        ["mesh-info", "meshInfo"],
        ["materials", "materials"],
        ["camera-matrices", "cameraMatrices"],
      ])
        addBuffer({ id, array, usage: ["storage"] });
      const forwardPipelines = new Set<string>();
      let firstRender = true;
      for (const mesh of renderedMeshes) {
        const vertex = `geometry-${mesh.geometryId}-positions`;
        addBuffer({
          id: vertex,
          array: `geometry.${mesh.geometryId}.positions`,
          usage: ["vertex"],
          sync: "loadout",
        });
        const normal = `geometry-${mesh.geometryId}-normals`;
        const uv = `geometry-${mesh.geometryId}-uvs`;
        const tangent = `geometry-${mesh.geometryId}-tangents`;
        const hasNormals = !!this.geometryData(mesh.geometryId, "normals");
        const hasUvs = !!this.geometryData(mesh.geometryId, "uvs");
        const hasTangents = !!this.geometryData(mesh.geometryId, "tangents");
        for (const [present, id, kind] of [
          [hasNormals, normal, "normals"],
          [hasUvs, uv, "uvs"],
          [hasTangents, tangent, "tangents"],
        ] as const)
          if (present)
            addBuffer({
              id,
              array: `geometry.${mesh.geometryId}.${kind}`,
              usage: ["vertex"],
              sync: "loadout",
            });
        const indexed = mesh.indexCount > 0;
        if (indexed)
          addBuffer({
            id: `geometry-${mesh.geometryId}-indices`,
            array: `geometry.${mesh.geometryId}.indices`,
            usage: ["index"],
            sync: "loadout",
          });
        const draws =
          indexed && mesh.faceMaterials.size
            ? Array.from(
                { length: Math.floor(mesh.indexCount / 3) },
                (_, face) => ({
                  face,
                  count: 3,
                  firstIndex: face * 3,
                  material: mesh.faceMaterials.get(face),
                }),
              )
            : [
                {
                  face: -1,
                  count: indexed ? mesh.indexCount : mesh.vertexCount,
                  firstIndex: 0,
                  material: undefined,
                },
              ];
        for (const draw of draws) {
          const id = `forward-${mesh.id}-${draw.face}`;
          const material =
            draw.material ?? Number(this.array("meshInfo").row(mesh.id)[1]);
          if (mesh.id > 65535 || material > 65534)
            throw new RangeError(
              `Scene handle limit: mesh ${mesh.id}, material ${material}`,
            );
          const pointers = this.array("materialTextures").row(material);
          const texture = (lane: number) =>
            pointers[lane]
              ? this.#textures.get(Number(pointers[lane]) - 1)
              : undefined;
          const baseTexture = texture(0);
          const materialTexture = texture(1);
          const normalTexture = hasTangents ? texture(2) : undefined;
          const detailed = hasNormals && hasUvs;
          const mask = detailed
            ? (baseTexture ? 1 : 0) |
              (materialTexture ? 2 : 0) |
              (normalTexture ? 4 : 0)
            : 0;
          const pipeline = detailed ? `forward-pbr-${mask}` : "forward-basic";
          if (!forwardPipelines.has(pipeline)) {
            forwardPipelines.add(pipeline);
            renderPipelines.push({
              id: pipeline,
              code: detailed ? pbrShader(mask) : basicForwardShader,
              vertex: {
                entry: "vertex",
                buffers: detailed
                  ? [
                      {
                        arrayStride: 16,
                        attributes: [
                          {
                            format: "float32x3",
                            offset: 0,
                            shaderLocation: 0,
                          },
                        ],
                      },
                      {
                        arrayStride: 16,
                        attributes: [
                          {
                            format: "float32x3",
                            offset: 0,
                            shaderLocation: 1,
                          },
                        ],
                      },
                      {
                        arrayStride: 16,
                        attributes: [
                          {
                            format: "float32x2",
                            offset: 0,
                            shaderLocation: 2,
                          },
                        ],
                      },
                      ...(normalTexture
                        ? [
                            {
                              arrayStride: 16,
                              attributes: [
                                {
                                  format: "float32x4",
                                  offset: 0,
                                  shaderLocation: 3,
                                },
                              ],
                            },
                          ]
                        : []),
                    ]
                  : [
                      {
                        arrayStride: 16,
                        attributes: [
                          {
                            format: "float32x3",
                            offset: 0,
                            shaderLocation: 0,
                          },
                        ],
                      },
                    ],
              },
              fragment: {
                entry: "fragment",
                targets: [{ format: hdrFormat }],
              },
              depthStencil: {
                format: "depth24plus",
                depth_write_enabled: true,
                depth_compare: "less",
              },
            });
          }
          const instance =
            (mesh.id +
              (draw.material === undefined
                ? 0
                : (draw.material + 1) * 65536)) >>>
            0;
          passes.push({
            id,
            type: "render",
            pipeline,
            after: previous,
            bindings: [
              ...[
                "clusters",
                "accent",
                "node-positions",
                "node-rotors",
                "node-scales",
                "mesh-info",
                "materials",
                "camera-matrices",
              ].map((resource, binding) => ({
                group: 0,
                binding,
                resource,
              })),
              ...(detailed && mask
                ? [
                    { group: 1, binding: 0, resource: "material-linear" },
                    ...(baseTexture
                      ? [
                          {
                            group: 1,
                            binding: 1,
                            resource: baseTexture.id,
                          },
                        ]
                      : []),
                    ...(materialTexture
                      ? [
                          {
                            group: 1,
                            binding: 2,
                            resource: materialTexture.id,
                          },
                        ]
                      : []),
                    ...(normalTexture
                      ? [
                          {
                            group: 1,
                            binding: 3,
                            resource: normalTexture.id,
                          },
                        ]
                      : []),
                  ]
                : []),
            ],
            color: [
              {
                resource: "hdr",
                ...(firstRender
                  ? { clear: [0.015, 0.025, 0.05, 1] }
                  : { load: "load" }),
              },
            ],
            depth: {
              resource: "depth",
              ...(firstRender ? { clear: 1 } : { load: "load" }),
            },
            vertexBuffers: [
              { slot: 0, resource: vertex },
              ...(detailed
                ? [
                    { slot: 1, resource: normal },
                    { slot: 2, resource: uv },
                    ...(normalTexture ? [{ slot: 3, resource: tangent }] : []),
                  ]
                : []),
            ],
            ...(indexed
              ? {
                  indexBuffer: {
                    resource: `geometry-${mesh.geometryId}-indices`,
                    format: "uint32",
                  },
                }
              : {}),
            draw: indexed
              ? {
                  indices: draw.count,
                  firstIndex: draw.firstIndex,
                  instances: 1,
                  firstInstance: instance,
                }
              : { vertices: draw.count, instances: 1, firstInstance: instance },
          });
          firstRender = false;
          previous = [id];
        }
      }
    }

    for (const material of this.#shaders.values()) {
      const id = `shader-${material.id}`;
      renderPipelines.push({
        id,
        code: material.code,
        vertex: { entry: material.vertexEntry },
        fragment: {
          entry: material.fragmentEntry,
          targets: [{ format: hdrFormat }],
        },
      });
      passes.push({
        id,
        type: "render",
        pipeline: id,
        after: previous,
        color: [{ resource: "hdr", load: "load" }],
        draw: { vertices: 3 },
      });
      previous = [id];
    }

    let input = "hdr";
    for (const [index, effect] of [...this.#effects.values()].entries()) {
      const output = `post-${index}`;
      const pipeline = `post-${effect.id}`;
      addTexture({
        id: output,
        format: hdrFormat,
        size: ["canvas", "canvas", 1],
        usage: ["render", "sampled"],
        transient: true,
      });
      renderPipelines.push({
        id: pipeline,
        code: effectFragment(effect.kind, effect.options),
        vertex: { entry: "vertex" },
        fragment: { entry: "fragment", targets: [{ format: hdrFormat }] },
      });
      passes.push({
        id: pipeline,
        type: "render",
        pipeline,
        after: previous,
        bindings: [
          { group: 0, binding: 0, resource: input },
          { group: 0, binding: 1, resource: "linear" },
        ],
        color: [{ resource: output, clear: [0, 0, 0, 1] }],
        draw: { vertices: 3 },
      });
      input = output;
      previous = [pipeline];
    }

    const toneMap = String(
      [...this.#effects.values()].find(
        (effect) => effect.kind === "colorGrading",
      )?.options.toneMap ?? "aces",
    );
    renderPipelines.push({
      id: "present",
      code: presentShader(toneMap),
      vertex: { entry: "vertex" },
      fragment: { entry: "fragment", targets: [{ format: "canvas" }] },
    });
    passes.push({
      id: "present",
      type: "render",
      pipeline: "present",
      after: previous,
      bindings: [
        { group: 0, binding: 0, resource: input },
        { group: 0, binding: 1, resource: "linear" },
      ],
      color: [{ resource: "canvas", clear: [0, 0, 0, 1] }],
      draw: { vertices: 3 },
    });

    const graph = {
      id: "scene",
      resources: {
        buffers: [...buffers.values()],
        textures: [...textures.values()],
        samplers: [...samplers.values()].map(({ id, ...descriptor }) => ({
          id,
          descriptor,
        })),
      },
      pipelines: { render: renderPipelines, compute: computePipelines },
      passes,
    };
    const id = await this.core.compileGraph(serialize(graph));
    await this.core.switchLoadout(id);
  }

  dispose() {
    this.core.dispose();
  }
}
