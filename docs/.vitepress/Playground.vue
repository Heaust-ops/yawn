<script setup>
import { basicSetup } from "codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import * as Handles from "@yawn/handles";
import { YawnCore } from "@yawn/core";
import { playgrounds } from "./playgrounds";

const props = defineProps({
  example: { type: String, default: "triangle" },
  fullscreen: { type: Boolean, default: false },
});
const examples = Object.entries(playgrounds);
const selected = ref(playgrounds[props.example] ? props.example : "triangle");
const preset = computed(() => playgrounds[selected.value]);
const canvas = ref();
const editor = ref();
const preview = ref();
const source = ref(preset.value.code);
const status = ref("Starting…");
const output = ref([]);
const failed = ref(false);
const running = ref(false);
const canvasKey = ref(0);
const fps = ref(0);
const profilerOpen = ref(false);
const profilerSupported = ref(null);
const profile = ref(null);
const adapterInfo = ref("");
let generation = 0;
let current;
let stopProfile;
let fpsFrame = 0;
let sampledFrame = 0;
let sampledAt = 0;
let editorView;

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const api = { ...Handles, YawnCore };

function updateSource(value) {
  source.value = value;
  if (editorView && editorView.state.doc.toString() !== value) {
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: value },
    });
  }
}

function requestedSave() {
  if (!props.fullscreen) return selected.value;
  const save = new URL(location.href).searchParams.get("save");
  return save && playgrounds[save] ? save : "triangle";
}

function openSave(event, save, push = true) {
  event?.preventDefault();
  selected.value = playgrounds[save] ? save : "triangle";
  updateSource(preset.value.code);
  if (props.fullscreen && push) {
    const url = new URL(location.href);
    url.searchParams.set("save", selected.value);
    history.pushState(null, "", url);
  }
  run();
}

function restoreSave() {
  openSave(undefined, requestedSave(), false);
}

async function dispose(value = current) {
  if (!value) return;
  if (value === current) {
    stopProfile?.();
    stopProfile = undefined;
    current = undefined;
    delete window.__yawnPlayground;
  }
  if (typeof value === "function") await value();
  else if (typeof value.dispose === "function") await value.dispose();
}

async function attachProfiler(runId = generation) {
  stopProfile?.();
  stopProfile = undefined;
  profile.value = null;
  profilerSupported.value = null;
  if (!profilerOpen.value) return;
  if (!adapterInfo.value) {
    const adapter = await navigator.gpu?.requestAdapter({
      powerPreference: "high-performance",
    });
    const info = adapter?.info;
    adapterInfo.value = [
      info?.vendor,
      info?.architecture,
      info?.device,
      info?.description,
    ]
      .filter(Boolean)
      .join(" · ");
  }
  const core = current?.scene?.core ?? current?.core;
  if (!core?.onProfile || !core?.setProfiler) {
    profilerSupported.value = false;
    return;
  }
  stopProfile = core.onProfile((stats) => {
    if (runId === generation) profile.value = stats;
  });
  profilerSupported.value = await core.setProfiler(true);
}

async function toggleProfiler() {
  profilerOpen.value = !profilerOpen.value;
  if (profilerOpen.value) {
    await attachProfiler();
  } else {
    stopProfile?.();
    stopProfile = undefined;
    profile.value = null;
    const core = current?.scene?.core ?? current?.core;
    await core?.setProfiler?.(false);
  }
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
    canvas.value.width = props.fullscreen
      ? Math.max(1, preview.value.clientWidth)
      : 960;
    canvas.value.height = props.fullscreen
      ? Math.max(1, preview.value.clientHeight)
      : 540;
    const executable = source.value.replace(/^\s*import\s+[^;]+;\s*$/gm, "");
    const names = Object.keys(api);
    const started = performance.now();
    const result = await new AsyncFunction(
      ...names,
      "canvas",
      "log",
      executable,
    )(...names.map((name) => api[name]), canvas.value, (message) =>
      runId === generation && output.value.push(String(message)),
    );
    if (runId !== generation) {
      await dispose(result);
      return;
    }
    current = result;
    window.__yawnPlayground = result;
    await attachProfiler(runId);
    status.value = `Running · ${Math.round(performance.now() - started)} ms`;
  } catch (error) {
    if (runId === generation) {
      failed.value = true;
      status.value = error instanceof Error ? error.message : String(error);
    }
  } finally {
    if (runId === generation) running.value = false;
  }
}

function reset() {
  updateSource(preset.value.code);
  run();
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
      const measured = ((frame - sampledFrame) * 1000) / (time - sampledAt);
      fps.value = measured < 10 ? measured.toFixed(1) : Math.round(measured);
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
  selected.value = requestedSave();
  source.value = preset.value.code;
  editorView = new EditorView({
    doc: source.value,
    parent: editor.value,
    extensions: [
      basicSetup,
      javascript(),
      oneDark,
      EditorView.lineWrapping,
      EditorView.contentAttributes.of({
        "aria-label": "Editable playground code",
        spellcheck: "false",
      }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) source.value = update.state.doc.toString();
      }),
    ],
  });
  if (props.fullscreen) addEventListener("popstate", restoreSave);
  sampleFps();
  run();
});
onUnmounted(() => {
  generation++;
  removeEventListener("popstate", restoreSave);
  editorView?.destroy();
  cancelAnimationFrame(fpsFrame);
  dispose();
});
</script>

<template>
  <section
    class="playground"
    :class="{ fullscreen }"
    :aria-label="`${preset.title} playground`"
  >
    <header>
      <a v-if="fullscreen" class="brand" href="/">Yawn</a>
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
      <button
        v-if="fullscreen"
        type="button"
        class="secondary"
        :aria-pressed="profilerOpen"
        data-profiler-toggle
        @click="toggleProfiler"
      >
        Profile
      </button>
      <button type="button" :disabled="running" @click="run">
        {{ running ? "Running…" : "Run" }}
      </button>
    </header>
    <nav v-if="fullscreen" class="saves" aria-label="Saved playgrounds">
      <span>Saved</span>
      <a
        v-for="([save, item]) in examples"
        :key="save"
        :href="`/playground?save=${save}`"
        :aria-current="save === selected ? 'page' : undefined"
        @click="openSave($event, save)"
      >
        {{ item.title }}
      </a>
    </nav>
    <div class="workspace">
      <div ref="editor" class="editor" />
      <div ref="preview" class="preview">
        <canvas :key="canvasKey" ref="canvas" aria-label="Yawn WebGPU output" />
        <aside v-if="profilerOpen" class="profiler" data-playground-profiler>
          <div class="profiler-title">
            <strong>GPU passes</strong>
            <span v-if="profile">{{ profile.milliseconds.toFixed(2) }} ms</span>
          </div>
          <div v-if="profile" class="profiler-meta">
            <span>{{ adapterInfo || profile.adapter }}</span>
            <span>
              {{ profile.canvas.width }}×{{ profile.canvas.height }} ·
              {{ profile.wallMilliseconds.toFixed(2) }} ms wall
            </span>
          </div>
          <p v-if="profilerSupported === false">
            Timestamp queries are unavailable on this GPU.
          </p>
          <p v-else-if="!profile">Waiting for a completed frame…</p>
          <table v-else>
            <tbody>
              <tr v-for="(pass, index) in profile.passes" :key="`${index}-${pass.name}`">
                <th>{{ pass.name }}</th>
                <td>{{ pass.milliseconds.toFixed(2) }} ms</td>
              </tr>
            </tbody>
          </table>
        </aside>
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
.playground.fullscreen {
  position: fixed;
  z-index: 100;
  inset: 0;
  left: 0;
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  height: 100dvh;
  margin: 0;
  transform: none;
  border: 0;
  border-radius: 0;
  box-shadow: none;
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
.brand {
  padding-right: 12px;
  color: #60a5fa;
  border-right: 1px solid #334155;
  font-weight: 800;
  letter-spacing: -0.02em;
  text-decoration: none;
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
.saves {
  display: flex;
  min-height: 34px;
  align-items: center;
  gap: 5px;
  overflow-x: auto;
  padding: 4px 10px;
  color: #64748b;
  border-bottom: 1px solid #263249;
  background: #0d1424;
  font:
    11px/1 ui-monospace,
    SFMono-Regular,
    Menlo,
    monospace;
  white-space: nowrap;
}
.saves span {
  margin-right: 3px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.saves a {
  padding: 5px 7px;
  color: #94a3b8;
  border-radius: 5px;
  text-decoration: none;
}
.saves a:hover {
  color: #e2e8f0;
  background: #1e293b;
}
.saves a[aria-current="page"] {
  color: #bfdbfe;
  background: #1d4ed8;
}
.workspace {
  display: grid;
  grid-template-columns: 1fr 1fr;
  min-height: 510px;
}
.fullscreen .workspace {
  min-height: 0;
  flex: 1;
}
.editor {
  min-width: 0;
  height: 510px;
  border-right: 1px solid #263249;
  background: #0b1020;
}
.fullscreen .editor {
  height: auto;
  min-height: 0;
}
.editor :deep(.cm-editor) {
  height: 100%;
  background: #0b1020;
  font:
    13px/1.55 ui-monospace,
    SFMono-Regular,
    Menlo,
    Consolas,
    monospace;
}
.editor :deep(.cm-scroller) {
  overflow: auto;
  font-family: inherit;
}
.editor :deep(.cm-gutters) {
  color: #526079;
  border-right-color: #263249;
  background: #0d1424;
}
.editor :deep(.cm-focused) {
  outline: none;
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
.profiler {
  position: absolute;
  z-index: 2;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(320px, 72%);
  overflow: auto;
  padding: 16px;
  color: #cbd5e1;
  border-left: 1px solid #334155;
  background: rgb(2 6 23 / 94%);
  font:
    12px/1.45 ui-monospace,
    SFMono-Regular,
    Menlo,
    monospace;
}
.profiler-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  color: #e2e8f0;
}
.profiler-title span {
  color: #60a5fa;
  font-weight: 700;
}
.profiler-meta {
  display: grid;
  gap: 3px;
  margin: -5px 0 12px;
  color: #64748b;
  font-size: 10px;
}
.profiler p {
  color: #94a3b8;
}
.profiler table {
  width: 100%;
  border-collapse: collapse;
}
.profiler th,
.profiler td {
  padding: 7px 0;
  border-bottom: 1px solid #1e293b;
}
.profiler th {
  overflow: hidden;
  max-width: 190px;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.profiler td {
  color: #93c5fd;
  text-align: right;
  white-space: nowrap;
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
  .editor {
    height: 360px;
    border-right: 0;
    border-bottom: 1px solid #263249;
  }
  .fullscreen .editor {
    height: 45vh;
    min-height: 240px;
  }
  .preview {
    min-height: 360px;
  }
  .fullscreen .preview {
    min-height: 0;
  }
  header strong {
    display: none;
  }
}
</style>
