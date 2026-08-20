<script setup>
import { onMounted, onUnmounted, ref } from "vue";
import {
  ComputePass,
  FXAA,
  Mesh,
  PBRMaterial,
  PointLight,
  Scene,
} from "@yawn/handles";

const props = defineProps({ example: { type: String, default: "triangle" } });

const canvas = ref();
const status = ref("Starting…");
const failed = ref(false);
let scene;
let accent;

function move(event) {
  if (!accent) return;
  const bounds = canvas.value.getBoundingClientRect();
  const row = accent.row(0);
  row[0] = (event.clientX - bounds.left) / bounds.width;
  row[1] = 1 - (event.clientY - bounds.top) / bounds.height;
}

onMounted(async () => {
  try {
    if (!crossOriginIsolated) throw new Error("Cross-origin isolation is disabled");
    canvas.value.width = 960;
    canvas.value.height = 540;
    scene = new Scene(canvas.value, { hdr: true });
    await scene.ready;
    accent = scene.array("sceneAccent");

    const material = new PBRMaterial(scene, {
      baseColor: props.example === "lights" ? [1, 0.55, 0.18, 1] : [0.75, 0.9, 1, 1],
      metallic: props.example === "materials" ? 0.9 : 0.1,
      roughness: 0.38,
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

    if (props.example === "instances") {
      mesh.position = [-0.45, 0, 0];
      mesh.scale = [0.65, 0.65, 1];
      const clone = mesh.clone({ position: [0.45, 0, 0], scale: [0.65, 0.65, 1] });
      await clone.ready;
    } else if (props.example === "lights") {
      await new PointLight(scene, { position: [0, 0.4, 0.2], color: [1, 0.4, 0.1], intensity: 8 }).ready;
    } else if (props.example === "post") {
      await new FXAA(scene).ready;
    } else if (props.example === "compute") {
      await scene.ensureRows("playgroundCompute", 1, 16, "u32");
      const compute = new ComputePass({
        id: "playground-compute",
        code: "@group(0) @binding(0) var<storage, read_write> value: array<u32>; @compute @workgroup_size(1) fn main() { value[0] = value[0] + 1u; }",
        buffers: [{ id: "playground-value", array: "playgroundCompute", usage: ["storage"] }],
        bindings: [{ group: 0, binding: 0, resource: "playground-value" }],
      });
      await scene.addComputePass(compute);
    }

    window.__yawnPlayground = { scene, mesh, material, accent };
    status.value = `${props.example} running · pointer movement writes sceneAccent in the SAB`;
  } catch (error) {
    failed.value = true;
    status.value = error.message;
  }
});

onUnmounted(() => {
  delete window.__yawnPlayground;
  scene?.dispose();
});
</script>

<template>
  <div class="playground">
    <canvas ref="canvas" aria-label="Yawn WebGPU output" @pointermove="move" />
    <p :class="{ failed }" data-playground-status>{{ status }}</p>
  </div>
</template>

<style scoped>
.playground { margin: 24px 0; }
canvas { width: 100%; aspect-ratio: 16 / 9; display: block; background: #111827; border-radius: 12px; }
p { color: var(--vp-c-text-2); }
.failed { color: var(--vp-c-danger-1); }
</style>
