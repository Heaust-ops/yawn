const triangle = `import { Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;

const material = new PBRMaterial(scene, {
  baseColor: [0.15, 0.55, 1, 1],
  metallic: 0.15,
  roughness: 0.4,
});
await material.ready;

const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

log("Move the pointer to write sceneAccent directly in the SAB.");
const move = (event) => {
  const bounds = canvas.getBoundingClientRect();
  scene.array("sceneAccent").row(0).set([
    (event.clientX - bounds.left) / bounds.width,
    1 - (event.clientY - bounds.top) / bounds.height,
    1,
    1,
  ]);
};
canvas.addEventListener("pointermove", move);

return {
  scene,
  mesh,
  dispose() {
    canvas.removeEventListener("pointermove", move);
    scene.dispose();
  },
};`;

const sab = `import { Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [0.2, 0.9, 0.65, 1] });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.35, -0.35, 0, 0.35, -0.35, 0, 0, 0.42, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

const velocity = await scene.ensureRows("app.velocity", 1, 16, "f32");
velocity.row(0).set([0.8, 0, 0, 0]);
log("Pointer movement mutates nodePositions; no worker message is sent.");

const move = (event) => {
  const bounds = canvas.getBoundingClientRect();
  mesh.position.x = ((event.clientX - bounds.left) / bounds.width - 0.5) * 1.4;
  mesh.position.y = (0.5 - (event.clientY - bounds.top) / bounds.height) * 1.2;
};
canvas.addEventListener("pointermove", move);

return {
  scene,
  mesh,
  dispose() {
    canvas.removeEventListener("pointermove", move);
    scene.dispose();
  },
};`;

const cameras = `import { ArcRotateCamera, Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [0.9, 0.3, 0.18, 1] });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

const camera = new ArcRotateCamera(scene, {
  target: mesh,
  alpha: 0,
  beta: Math.PI / 2,
  radius: 3,
  fov: Math.PI / 3,
  near: 0.05,
  far: 100,
  aspect: canvas.width / canvas.height,
  controls: { element: canvas, pointer: true, controller: true },
});
await camera.ready;
log("Drag to orbit, right-drag to pan, and wheel to zoom.");

return {
  scene,
  mesh,
  camera,
  async dispose() {
    await camera.dispose();
    scene.dispose();
  },
};`;

const instances = `import { Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [0.25, 0.8, 1, 1] });
await material.ready;
const source = new Mesh(scene, {
  position: [-0.48, 0, 0],
  scale: [0.58, 0.58, 1],
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await source.ready;

const instance = source.clone({ position: [0.48, 0, 0], scale: [0.58, 0.58, 1] });
await instance.ready;
log(\`Two mesh handles share geometry #\${source.geometryId}.\`);

return { scene, source, instance, dispose: () => scene.dispose() };`;

const materials = `import { Mesh, PBRMaterial, Scene, Texture } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const pixels = new OffscreenCanvas(64, 64);
const context = pixels.getContext("2d");
context.fillStyle = "#ff3018";
context.fillRect(0, 0, 64, 64);
context.fillStyle = "#ffd166";
context.fillRect(0, 0, 32, 32);
context.fillRect(32, 32, 32, 32);
const albedo = new Texture(scene, {
  source: pixels.transferToImageBitmap(),
  format: "rgba8unorm-srgb",
});
await albedo.ready;
const paint = new PBRMaterial(scene, {
  baseColor: [1, 1, 1, 1],
  baseColorTexture: albedo,
  metallic: 0.75,
  roughness: 0.18,
});
await paint.ready;
const mesh = new Mesh(scene, {
  material: paint,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
    uvs: [0, 0, 2, 0, 1, 2],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

log("Move horizontally to mutate material roughness in shared memory.");
const move = (event) => {
  const bounds = canvas.getBoundingClientRect();
  paint.roughness = (event.clientX - bounds.left) / bounds.width;
};
canvas.addEventListener("pointermove", move);

return {
  scene,
  mesh,
  paint,
  albedo,
  dispose() {
    canvas.removeEventListener("pointermove", move);
    scene.dispose();
  },
};`;

const lights = `import { AmbientLight, Mesh, PBRMaterial, PointLight, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [1, 0.45, 0.08, 1], roughness: 0.35 });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

const point = new PointLight(scene, {
  position: [0, 0.5, 0.5],
  color: [1, 0.18, 0.04],
  intensity: 12,
  range: 8,
});
const ambient = new AmbientLight(scene, { color: [0.04, 0.12, 0.3], intensity: 0.35 });
await Promise.all([point.ready, ambient.ready]);
log("Point and ambient rows are consumed by the clustered compute pass.");

return { scene, mesh, point, ambient, dispose: () => scene.dispose() };`;

const compute = `import { ComputePass, Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [0.15, 0.75, 1, 1] });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

const values = await scene.ensureRows("simulation.values", 1, 16, "u32");
const simulation = new ComputePass({
  id: "increment",
  code: "@group(0) @binding(0) var<storage, read_write> values: array<u32>; @compute @workgroup_size(1) fn main() { values[0] += 1u; }",
  buffers: [{ id: "simulation-values", array: "simulation.values", usage: ["storage"] }],
  bindings: [{ group: 0, binding: 0, resource: "simulation-values" }],
});
await scene.addComputePass(simulation);
await new Promise((resolve) => setTimeout(resolve, 100));
log(\`Compute wrote \${values.row(0)[0]} into the SAB-backed buffer.\`);

return { scene, mesh, simulation, dispose: () => scene.dispose() };`;

const post = `import { ColorGrading, DynamicExposure, FXAA, Mesh, PBRMaterial, Scene } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
const material = new PBRMaterial(scene, { baseColor: [0.8, 0.18, 1, 1] });
await material.ready;
const mesh = new Mesh(scene, {
  material,
  vertexData: {
    positions: [-0.7, -0.6, 0, 0.7, -0.6, 0, 0, 0.72, 0],
    indices: [0, 1, 2],
  },
});
await mesh.ready;

let exposure, grade, fxaa;
await scene.batchGraphUpdates(async () => {
  exposure = new DynamicExposure(scene, { exposure: 1.25 });
  grade = new ColorGrading(scene, { toneMap: "aces", amount: 1 });
  fxaa = new FXAA(scene);
  await Promise.all([exposure.ready, grade.ready, fxaa.ready]);
});
log("HDR → exposure → color grading → FXAA → canvas");

return { scene, mesh, exposure, grade, fxaa, dispose: () => scene.dispose() };`;

const importing = `import { AmbientLight, ArcRotateCamera, PointLight, Picking, Scene, importGltf } from "@yawn/handles";

const scene = new Scene(canvas, { hdr: true, arenaBytes: 384 * 1024 * 1024 });
await scene.ready;
scene.array("sceneAccent").row(0).set([1, 1, 1, 1]);
log("Importing /models/sponza.glb in the importer worker…");
const meshes = await importGltf(scene, "/models/sponza.glb");

// Add a warm ambient/key/fill setup without changing the authored transforms.
const lights = [
  new AmbientLight(scene, {
    color: [1, 0.93, 0.82],
    intensity: 0.18,
  }),
  new PointLight(scene, {
    position: [-10, 14, -4],
    color: [1, 0.72, 0.5],
    intensity: 2,
    range: 28,
  }),
  new PointLight(scene, {
    position: [9, 10, 6],
    color: [1, 0.9, 0.75],
    intensity: 1.5,
    range: 24,
  }),
];
await Promise.all(lights.map((light) => light.ready));

const target = [0, 8, 0];
const camera = new ArcRotateCamera(scene, {
  targetPosition: target,
  alpha: Math.PI / 2,
  beta: Math.PI / 3,
  radius: 30,
  near: 1,
  far: 1000,
  aspect: canvas.width / canvas.height,
  controls: { element: canvas, pointer: true },
});
await camera.ready;

const picking = new Picking(scene);
await picking.ready;
const pick = async () => {
  const origin = Array.from(camera.position);
  const direction = target.map((value, lane) => value - origin[lane]);
  const length = Math.hypot(...direction);
  const hits = await picking.pick(origin, direction.map((value) => value / length));
  log(\`Imported \${meshes.length} primitives; BVH ray returned \${hits.length} hit(s).\`);
};
canvas.addEventListener("click", pick);
await pick();

return {
  scene,
  meshes,
  lights,
  camera,
  picking,
  async dispose() {
    canvas.removeEventListener("click", pick);
    picking.dispose();
    await camera.dispose();
    scene.dispose();
  },
};`;

const benchmark = `import { ArcRotateCamera, Mesh, PBRMaterial, Scene, Texture } from "@yawn/handles";

const parameters = new URL(location.href).searchParams;
const draws = Math.max(1, Math.min(512, Number(parameters.get("draws")) || 138));
const grid = Math.max(1, Math.min(256, Number(parameters.get("grid")) || 128));
const overdraw = parameters.has("overdraw");
const columns = 12;
const positions = [];
const normals = [];
const uvs = [];
const indices = [];
for (let y = 0; y <= grid; y++) {
  for (let x = 0; x <= grid; x++) {
    positions.push(x / grid * 2 - 1, y / grid * 2 - 1, 0);
    normals.push(0, 0, 1);
    uvs.push(x / grid * 32, y / grid * 32);
  }
}
for (let y = 0; y < grid; y++) {
  for (let x = 0; x < grid; x++) {
    const first = y * (grid + 1) + x;
    indices.push(
      first, first + 1, first + grid + 2,
      first, first + grid + 2, first + grid + 1,
    );
  }
}

const pixels = new OffscreenCanvas(1024, 1024);
const context = pixels.getContext("2d");
for (let y = 0; y < 64; y++) {
  for (let x = 0; x < 64; x++) {
    context.fillStyle = (x + y) % 2 ? "#e2e8f0" : "#172554";
    context.fillRect(x * 16, y * 16, 16, 16);
  }
}

const scene = new Scene(canvas, { hdr: true });
await scene.ready;
log("Benchmark core ready; preparing geometry…");
const camera = new ArcRotateCamera(scene, {
  targetPosition: [0, 0, 0],
  alpha: 0,
  beta: Math.PI / 2,
  radius: 3,
  aspect: canvas.width / canvas.height,
});
await camera.ready;
log("Benchmark camera ready; compiling graph…");
let material;
let source;
const meshes = [];
await scene.batchGraphUpdates(async () => {
  const texture = new Texture(scene, {
    source: pixels.transferToImageBitmap(),
    format: "rgba8unorm-srgb",
  });
  await texture.ready;
  material = new PBRMaterial(scene, {
    baseColorTexture: texture,
    roughness: 0.45,
  });
  await material.ready;
  source = new Mesh(scene, {
    material,
    vertexData: { positions, normals, uvs, indices },
  });
  await source.ready;
  meshes.push(source);
  for (let index = 1; index < draws; index++) meshes.push(source.clone());
  await Promise.all(meshes.slice(1).map((mesh) => mesh.ready));
  for (let index = 0; index < meshes.length; index++) {
    const column = index % columns;
    const row = Math.floor(index / columns);
    meshes[index].position = overdraw
      ? [0, 0, index * 0.01]
      : [
          -1 + (column + 0.5) * 2 / columns,
          1 - (row + 0.5) * 2 / columns,
          0,
        ];
    meshes[index].scale = overdraw
      ? [0.9, 0.9, 1]
      : [0.9 / columns, 0.9 / columns, 1];
  }
});

const triangles = draws * grid * grid * 2;
log(\`Deterministic \${overdraw ? "overdraw" : "geometry"} benchmark: \${draws} draws, \${triangles.toLocaleString()} triangles, 1024² minified texture.\`);
log("Open Profile and compare Forward GPU time, not page load time.");

return { scene, meshes, material, camera, dispose: () => scene.dispose() };`;

const core = `import { YawnCore } from "@yawn/core";

const encode = (value) => {
  if (value === null || ["boolean", "number"].includes(typeof value)) return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return \`(array\${value.map((item) => \` \${encode(item)}\`).join("")})\`;
  return \`(object\${Object.keys(value).sort().map((key) =>
    \` (field \${JSON.stringify(key)} \${encode(value[key])})\`).join("")})\`;
};

const core = new YawnCore(canvas);
await core.ready;
const accent = await core.createRows({ name: "accent", rows: 1, stride: 16, format: "f32" });
accent.write(0, [0.2, 0.75, 1, 1]);

const code = "struct Out { @builtin(position) position: vec4<f32> }; @group(0) @binding(0) var<uniform> color: vec4<f32>; @vertex fn vertex(@builtin(vertex_index) id: u32) -> Out { let points = array(vec2(-.7,-.6), vec2(.7,-.6), vec2(0.,.72)); var out: Out; out.position = vec4(points[id],0.,1.); return out; } @fragment fn fragment() -> @location(0) vec4<f32> { return color; }";
const graph = {
  id: "direct-core",
  resources: { buffers: [{ id: "accent", array: "accent", usage: ["uniform"] }], textures: [], samplers: [] },
  pipelines: { render: [{ id: "triangle", code, vertex: { entry: "vertex" }, fragment: { entry: "fragment", targets: [{ format: "canvas" }] } }], compute: [] },
  passes: [{ id: "triangle", type: "render", pipeline: "triangle", bindings: [{ group: 0, binding: 0, resource: "accent" }], color: [{ resource: "canvas", clear: [0.01, 0.02, 0.04, 1] }], draw: { vertices: 3 } }],
};
const id = await core.compileGraph(\`(yawn-graph 1 \${encode(graph)})\`);
await core.switchLoadout(id);
log("The canvas is rendered by a graph sent directly to the Rust/WASM core.");

return { core, accent, dispose: () => core.dispose() };`;

export const playgrounds = {
  triangle: { title: "First scene", code: triangle },
  sab: { title: "Direct shared-memory movement", code: sab },
  cameras: { title: "Arc rotate camera", code: cameras },
  instances: { title: "Geometry instances", code: instances },
  materials: { title: "PBR material", code: materials },
  lights: { title: "Clustered lights", code: lights },
  compute: { title: "Compute pass", code: compute },
  post: { title: "HDR post processing", code: post },
  importing: { title: "glTF import and BVH picking", code: importing },
  benchmark: { title: "Forward benchmark", code: benchmark },
  core: { title: "Direct core graph", code: core },
};
