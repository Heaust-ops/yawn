export const PLAYGROUND_RECIPES = Object.freeze({
  "first-scene": Object.freeze({
    title: "Your first scene",
    package: "All packages",
    description: "Boot core, load procedural glTF through the import worker, and activate a graph.",
    docs: "/docs/guide/first-scene",
    source: `const scene = await yawn.createScene({ loadout: "cubes" });
yawn.status(
  \`Ready · \${scene.meshes.length} meshes · \${scene.core.telemetry.draws} draws\`,
);`,
  }),
  "jso-graph": Object.freeze({
    title: "Load a JSO render graph",
    package: "@yawn/render-graph-js",
    description: "Compile a plain-object graph through the canonical AST boundary.",
    docs: "/docs/packages/render-graph#plain-object-authoring",
    source: `const scene = await yawn.createScene({
  loadout: "spheres",
  graph: yawn.graphs.culling,
});
yawn.status(
  \`Graph \${scene.compiled.graphId} · \${scene.core.telemetry.draws} draws\`,
);`,
  }),
  "gltf-worker": Object.freeze({
    title: "Import glTF in a worker",
    package: "@yawn/gltf-import",
    description: "Stage a generated GLB in shared memory and commit only metadata.",
    docs: "/docs/packages/gltf-import",
    source: `const scene = await yawn.createScene({ loadout: "spheres" });
yawn.status(
  \`Imported \${scene.meshes.length} mesh handles through shared memory\`,
);`,
  }),
  "shared-animation": Object.freeze({
    title: "Animate through the SAB",
    package: "@yawn/core",
    description: "Write a generation-guarded instance transform every frame without messages.",
    docs: "/docs/packages/core#fast-path-shared-writes",
    source: `const scene = await yawn.createScene({ loadout: "cubes" });
const instance = scene.meshes[0].defaultInstance;
const start = performance.now();

function animate(now) {
  const angle = (now - start) * 0.001;
  instance.setTransform(yawn.rotationY(angle));
  requestAnimationFrame(animate);
}
requestAnimationFrame(animate);
yawn.status("Animating instance.transform directly in shared memory");`,
  }),
  "custom-soa": Object.freeze({
    title: "Allocate custom render data",
    package: "@yawn/core",
    description: "Add one aligned velocity row for every instance slot.",
    docs: "/docs/packages/core#request-an-soa-column",
    source: `const scene = await yawn.createScene({ loadout: "cubes" });
const velocity = await scene.core.allocateArray({
  name: "instance.velocity",
  domain: "instance",
  scalar: "f32",
  lanes: 4,
});

for (const mesh of scene.meshes) {
  velocity.write(mesh.defaultInstance.handle[0], [0, 0.25, 0, 0]);
}
yawn.status(\`Allocated \${velocity.length} SIMD-aligned velocity rows\`);`,
  }),
  "conventional-handles": Object.freeze({
    title: "Camera and material handles",
    package: "@yawn/mesh-handles",
    description: "Use familiar properties while mutations remain direct shared-memory writes.",
    docs: "/docs/packages/mesh-handles#camera-and-material-handles",
    source: `const scene = await yawn.createScene({ loadout: "materials" });
scene.camera.lookAt([11, 9, 13], [0, 0, 0]);

const material = scene.materials[1];
material.baseColor = [0.1, 0.55, 1, 1];
material.metallic = 0.15;
material.roughness = 0.28;
yawn.status("Camera and material properties committed through SAB rows");`,
  }),
  picking: Object.freeze({
    title: "Pick shared scene data",
    package: "@yawn/mesh-handles",
    description: "Build the optional worker-side BVH and query the closest instance.",
    docs: "/docs/packages/mesh-handles#worker-side-picking",
    source: `const scene = await yawn.createScene({ loadout: "cubes" });
const state = scene.camera.state;
const origin = state.slice(0, 3);
const direction = state.slice(4, 7).map((value, axis) => value - origin[axis]);
const result = await scene.handles.pickRay(origin, direction, { maxHits: 1 });
yawn.status(
  result.hits.length
    ? \`Picked instance \${result.hits[0].instance.handle.join(":")}\`
    : "No instance intersected the center ray",
);`,
  }),
});

export function playgroundRecipe(id) {
  return PLAYGROUND_RECIPES[id] ?? PLAYGROUND_RECIPES["first-scene"];
}
