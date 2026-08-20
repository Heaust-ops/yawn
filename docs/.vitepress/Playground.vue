<script setup>
import { onMounted, onUnmounted, ref } from "vue";
import { YawnCore } from "@yawn/core";
import { loadGraph } from "@yawn/render-graph-js";
import { triangleGraph } from "@yawn/default-pipelines";

const canvas = ref();
const status = ref("Starting…");
const failed = ref(false);
let core;
let color;

function move(event) {
  if (!color) return;
  const bounds = canvas.value.getBoundingClientRect();
  const row = color.row(0);
  row[0] = (event.clientX - bounds.left) / bounds.width;
  row[1] = 1 - (event.clientY - bounds.top) / bounds.height;
}

onMounted(async () => {
  try {
    if (!crossOriginIsolated) throw new Error("Cross-origin isolation is disabled");
    canvas.value.width = 960;
    canvas.value.height = 540;
    core = new YawnCore(canvas.value);
    await core.ready;
    color = await core.createRows({
      name: "triangle.color",
      rows: 1,
      stride: 16,
      format: "f32",
    });
    color.write(0, [0.2, 0.65, 1, 1]);
    await loadGraph(core, triangleGraph());
    window.__yawnPlayground = { core, color };
    status.value = "Running · move the pointer to write the shared color row";
  } catch (error) {
    failed.value = true;
    status.value = error.message;
  }
});

onUnmounted(() => {
  delete window.__yawnPlayground;
  core?.dispose();
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
