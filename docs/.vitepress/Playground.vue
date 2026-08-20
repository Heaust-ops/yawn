<script setup>
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import * as Handles from "@yawn/handles";
import { YawnCore } from "@yawn/core";
import { playgrounds } from "./playgrounds";

const props = defineProps({ example: { type: String, default: "triangle" } });
const preset = playgrounds[props.example] ?? playgrounds.triangle;
const canvas = ref();
const source = ref(preset.code);
const status = ref("Starting…");
const output = ref([]);
const failed = ref(false);
const running = ref(false);
const canvasKey = ref(0);
const fps = ref(0);
let generation = 0;
let current;
let fpsFrame = 0;
let sampledFrame = 0;
let sampledAt = 0;

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const api = { ...Handles, YawnCore };

async function dispose(value = current) {
  if (!value) return;
  current = undefined;
  delete window.__yawnPlayground;
  if (typeof value === "function") await value();
  else if (typeof value.dispose === "function") await value.dispose();
}

async function run() {
  const runId = ++generation;
  running.value = true;
  failed.value = false;
  output.value = [];
  status.value = "Running…";
  try {
    await dispose();
    if (!crossOriginIsolated)
      throw new Error("Cross-origin isolation is disabled");
    canvasKey.value++;
    await nextTick();
    canvas.value.width = 960;
    canvas.value.height = 540;
    const executable = source.value.replace(/^\s*import\s+[^;]+;\s*$/gm, "");
    const names = Object.keys(api);
    const started = performance.now();
    const result = await new AsyncFunction(
      ...names,
      "canvas",
      "log",
      executable,
    )(...names.map((name) => api[name]), canvas.value, (message) =>
      output.value.push(String(message)),
    );
    if (runId !== generation) {
      await dispose(result);
      return;
    }
    current = result;
    window.__yawnPlayground = result;
    status.value = `Running · ${Math.round(performance.now() - started)} ms`;
  } catch (error) {
    failed.value = true;
    status.value = error instanceof Error ? error.message : String(error);
  } finally {
    if (runId === generation) running.value = false;
  }
}

function reset() {
  source.value = preset.code;
  run();
}

function tab(event) {
  if (event.key !== "Tab") return;
  event.preventDefault();
  const editor = event.currentTarget;
  const start = editor.selectionStart;
  source.value = `${source.value.slice(0, start)}  ${source.value.slice(editor.selectionEnd)}`;
  requestAnimationFrame(() => editor.setSelectionRange(start + 2, start + 2));
}

function sampleFps(time = performance.now()) {
  try {
    const core = current?.scene?.core ?? current?.core;
    if (!core) throw new Error();
    const frame = Number(core.array("info").row(0)[1]);
    if (frame < sampledFrame) sampledAt = 0;
    if (!sampledAt) {
      sampledFrame = frame;
      sampledAt = time;
    } else if (time - sampledAt >= 500) {
      fps.value = Math.round(
        ((frame - sampledFrame) * 1000) / (time - sampledAt),
      );
      sampledFrame = frame;
      sampledAt = time;
    }
  } catch {
    fps.value = 0;
    sampledFrame = 0;
    sampledAt = time;
  }
  fpsFrame = requestAnimationFrame(sampleFps);
}

onMounted(() => {
  sampleFps();
  run();
});
onUnmounted(() => {
  generation++;
  cancelAnimationFrame(fpsFrame);
  dispose();
});
</script>

<template>
  <section class="playground" :aria-label="`${preset.title} playground`">
    <header>
      <strong>{{ preset.title }}</strong>
      <span :class="{ failed }" data-playground-status>{{ status }}</span>
      <button
        type="button"
        class="secondary"
        :disabled="running"
        @click="reset"
      >
        Reset
      </button>
      <button type="button" :disabled="running" @click="run">
        {{ running ? "Running…" : "Run" }}
      </button>
    </header>
    <div class="workspace">
      <textarea
        v-model="source"
        aria-label="Editable playground code"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        @keydown="tab"
      />
      <div class="preview">
        <canvas :key="canvasKey" ref="canvas" aria-label="Yawn WebGPU output" />
        <div v-if="output.length" class="output" data-playground-log>
          <div v-for="(line, index) in output" :key="index">{{ line }}</div>
        </div>
        <div class="fps" data-playground-fps>{{ fps }} FPS</div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.playground {
  position: relative;
  left: 50%;
  width: min(1120px, calc(100vw - 64px));
  margin: 28px 0 36px;
  overflow: hidden;
  transform: translateX(-50%);
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: #0b1020;
  box-shadow: var(--vp-shadow-3);
}
:global(.VPContent.has-sidebar .playground) {
  left: calc(50% + var(--vp-sidebar-width) / 2);
  width: min(1120px, calc(100vw - var(--vp-sidebar-width) - 64px));
}
@media (min-width: 1280px) {
  :global(.VPDoc.has-sidebar.has-aside .playground) {
    left: 50%;
    width: min(1120px, calc(100vw - var(--vp-sidebar-width) - 288px));
  }
}
header {
  display: flex;
  min-height: 46px;
  align-items: center;
  gap: 12px;
  padding: 7px 10px 7px 16px;
  color: #dbeafe;
  border-bottom: 1px solid #263249;
  background: #111827;
}
header strong {
  white-space: nowrap;
}
header span {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  color: #94a3b8;
  font:
    12px/1.4 ui-monospace,
    SFMono-Regular,
    Menlo,
    monospace;
  text-overflow: ellipsis;
  white-space: nowrap;
}
button {
  padding: 6px 14px;
  color: white;
  border: 0;
  border-radius: 6px;
  background: #2563eb;
  cursor: pointer;
  font-weight: 600;
}
button.secondary {
  color: #cbd5e1;
  background: #293449;
}
button:disabled {
  cursor: wait;
  opacity: 0.55;
}
.workspace {
  display: grid;
  grid-template-columns: 1fr 1fr;
  min-height: 510px;
}
textarea {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
  height: 510px;
  resize: none;
  padding: 18px;
  color: #dbeafe;
  border: 0;
  border-right: 1px solid #263249;
  outline: none;
  background: #0b1020;
  tab-size: 2;
  font:
    13px/1.55 ui-monospace,
    SFMono-Regular,
    Menlo,
    Consolas,
    monospace;
}
textarea:focus {
  box-shadow: inset 0 0 0 2px #2563eb;
}
.preview {
  position: relative;
  min-width: 0;
  overflow: hidden;
  background: #050914;
}
canvas {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: contain;
}
.output {
  position: absolute;
  right: 12px;
  bottom: 48px;
  left: 12px;
  max-height: 30%;
  overflow: auto;
  padding: 8px 10px;
  color: #bfdbfe;
  border: 1px solid #334155;
  border-radius: 6px;
  background: rgb(2 6 23 / 82%);
  font:
    11px/1.45 ui-monospace,
    SFMono-Regular,
    Menlo,
    monospace;
}
.fps {
  position: absolute;
  right: 12px;
  bottom: 12px;
  padding: 5px 9px;
  color: #dbeafe;
  border: 1px solid #334155;
  border-radius: 999px;
  background: rgb(2 6 23 / 82%);
  font:
    700 12px/1 ui-monospace,
    SFMono-Regular,
    Menlo,
    monospace;
}
.failed {
  color: #fca5a5;
}
@media (max-width: 900px) {
  .playground,
  :global(.VPContent.has-sidebar .playground) {
    left: auto;
    width: 100%;
    transform: none;
  }
  .workspace {
    grid-template-columns: 1fr;
  }
  textarea {
    height: 360px;
    border-right: 0;
    border-bottom: 1px solid #263249;
  }
  .preview {
    min-height: 360px;
  }
  header strong {
    display: none;
  }
}
</style>
