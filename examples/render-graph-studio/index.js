import { YawnCore, RendererError } from "@yawn/core";
import { MeshHandles } from "@yawn/mesh-handles";
import { installCameraRenderDataControls } from "../cookbook/16-camera-render-data.js";
import { loadDemoLoadout } from "./demo-loadouts.js";
import { adaptFxNodeSnapshot } from "@yawn/render-graph-fxnode";
import { createGraphAst } from "@yawn/render-graph-ast";
import { loadGraph } from "@yawn/render-graph-js";
import { GltfImporter } from "@yawn/gltf-import";
import { defaultPipelines } from "@yawn/default-pipelines";
import { AuthoringController } from "./render-graph/authoring-controller.js";
import { createRenderGraphEditor } from "./render-graph/fxnode-editor.js";
import { renderGraphPresets } from "./render-graph/presets.js";

let renderer,
  meshHandles,
  gltfImporter,
  editor,
  controller,
  assetAbort,
  busy = false,
  cleaned = false;
let unsubscribeController = () => {},
  unsubscribeSnapshots = () => {};
const listeners = [];
const on = (target, type, fn) => {
  target.addEventListener(type, fn);
  listeners.push(() => target.removeEventListener(type, fn));
};
const status = (message) => {
  const node = document.querySelector("#demo-status");
  if (node) node.textContent = message;
};
const sameId = (a, b) =>
  Array.isArray(a) && Array.isArray(b) && a[0] === b[0] && a[1] === b[1];
const state = {
  loadout: "cubes",
  graph: "authored",
  compiled: {},
  telemetry: null,
};
function createWorkerTransport() {
  const canvas = document.querySelector("#canvas0");
  const dpr = devicePixelRatio;
  canvas.width = Math.round(Math.max(1, canvas.clientWidth) * dpr);
  canvas.height = Math.round(Math.max(1, canvas.clientHeight) * dpr);

  const worker = new Worker(
    new URL(
      "../../renderer/src/platform/web/worker/mainWorker.js",
      import.meta.url,
    ),
    { type: "module", name: "yawn-renderer" },
  );
  const abort = new AbortController();
  const options = { signal: abort.signal };
  const post = (kind, values) =>
    worker.postMessage({
      type: "window-event",
      kind,
      values: new Float64Array(values),
    });
  addEventListener(
    "resize",
    () =>
      post(0, [
        Math.max(1, canvas.clientWidth),
        Math.max(1, canvas.clientHeight),
        devicePixelRatio,
      ]),
    options,
  );
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ type: "init", canvas: offscreen }, [offscreen]);
  return {
    worker,
    free() {
      abort.abort();
    },
  };
}

function publish(telemetry) {
  state.telemetry = telemetry;
  document.documentElement.dataset.yawnState = JSON.stringify({
    activeLoadout: state.loadout,
    activeGraph: state.graph,
    renderDataRevision: telemetry.revision,
    renderMode: telemetry.renderMode,
    activeCompiledId: telemetry.activeCompiledId,
    activeCompiledGraph: telemetry.activeCompiledGraph,
    activeCompiledRevision: telemetry.activeCompiledRevision,
    activeCompiledSchemaVersion: telemetry.activeCompiledSchemaVersion,
    graphExecutions: telemetry.graphExecutions,
    graphTextureSlots: telemetry.graphTextureSlots,
    draws: telemetry.draws,
    instances: telemetry.instances,
    indices: telemetry.indices,
    gpuError: telemetry.gpuError,
  });
}
function waitTelemetry(predicate, timeout = 30000) {
  const current = renderer?.telemetry;
  if (current && predicate(current)) return Promise.resolve(current);
  return new Promise((resolve, reject) => {
    let timer;
    const done = () => {
      clearTimeout(timer);
      renderer.removeEventListener("renderer-frame", frame);
    };
    const frame = (e) => {
      if (predicate(e.detail)) {
        done();
        resolve(e.detail);
      }
    };
    timer = setTimeout(() => {
      done();
      reject(new Error("Telemetry confirmation timed out"));
    }, timeout);
    onAbort = () => {
      done();
      reject(new RendererError("DISPOSED"));
    };
    renderer.addEventListener("renderer-frame", frame);
    timer.unref?.();
  });
}
let onAbort = () => {};
async function transaction(label, operation, rollback) {
  if (busy || cleaned) return false;
  busy = true;
  document
    .querySelectorAll("select, #apply-graph")
    .forEach((x) => (x.disabled = true));
  status(label);
  try {
    const telemetry = await operation();
    if (cleaned) return false;
    if (telemetry) {
      publish(telemetry);
      status(
        `${state.loadout} · ${state.graph} · ${telemetry.draws} draws · ${telemetry.instances} instances`,
      );
    } else
      status(
        `${state.loadout} · ${state.graph} · committed; telemetry pending`,
      );
    return true;
  } catch (error) {
    if (!cleaned) {
      try {
        await rollback?.();
      } catch (rollbackError) {
        console.error("Render graph rollback failed", rollbackError);
      }
      console.error("Render graph transaction failed", error);
      status(`Failed · ${error?.code ?? error?.message ?? error}`);
    }
    return false;
  } finally {
    busy = false;
    if (!cleaned) {
      document.querySelectorAll("select").forEach((x) => (x.disabled = false));
      const button = document.querySelector("#apply-graph");
      if (button) button.disabled = !controller?.canApply;
    }
  }
}
async function selectLoadout(next, select) {
  const previous = state.loadout,
    targetRevision = (renderer.telemetry?.revision ?? 0) + 1;
  assetAbort = new AbortController();
  const ok = await transaction(`Loading ${next}…`, async () => {
    const glb = await loadDemoLoadout(next, { signal: assetAbort.signal });
    const url = URL.createObjectURL(new Blob([glb], { type: "model/gltf-binary" }));
    try {
      meshHandles.fromImportedScene(await gltfImporter.load(url));
    } finally {
      URL.revokeObjectURL(url);
    }
    state.loadout = next;
    return waitTelemetry(
      (x) =>
        x.revision === targetRevision &&
        x.draws > 0 &&
        x.activeCompiledGraph === state.compiled[state.graph].graphId &&
        x.gpuError === false,
    ).catch(() => null);
  });
  assetAbort = undefined;
  if (!ok) select.value = previous;
}
async function selectGraph(next, select) {
  const previous = state.graph,
    compiled = state.compiled[next];
  const ok = await transaction(
    `Activating ${next}…`,
    async () => {
      await renderer.switchCompiledGraph(compiled.compiledId);
      state.graph = next;
      return waitTelemetry(
        (x) =>
          sameId(x.activeCompiledId, compiled.compiledId) &&
          x.activeCompiledGraph === compiled.graphId &&
          x.activeCompiledRevision === compiled.revision &&
          x.gpuError === false,
      ).catch(() => null);
    },
    async () => {
      state.graph = previous;
      select.value = previous;
    },
  );
  if (!ok) select.value = previous;
}
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  removeEventListener("pagehide", pagehide);
  assetAbort?.abort();
  onAbort();
  listeners.splice(0).forEach((fn) => fn());
  unsubscribeController();
  unsubscribeSnapshots();
  try {
    await controller?.destroy();
    await editor?.destroy();
  } finally {
    gltfImporter?.dispose();
    meshHandles?.dispose();
    renderer?.dispose();
  }
}
const pagehide = () => {
  void cleanup();
};

async function start() {
  addEventListener("pagehide", pagehide, { once: true });
  delete document.documentElement.dataset.yawnReady;
  const transport = createWorkerTransport();
  renderer = new YawnCore(transport);
  meshHandles = new MeshHandles(renderer);
  gltfImporter = new GltfImporter(renderer);
  await renderer.ready;
  listeners.push(
    installCameraRenderDataControls(renderer, document.querySelector("#canvas0")),
  );
  const nextEditor = await createRenderGraphEditor(
    document.querySelector("#graph-editor"),
  );
  if (cleaned) {
    await nextEditor.destroy();
    return;
  }
  editor = nextEditor;
  controller = new AuthoringController({
    renderer,
    adapt: (snapshot, revision) =>
      adaptFxNodeSnapshot(snapshot, revision, { pipelines: defaultPipelines }),
  });
  const apply = document.querySelector("#apply-graph"),
    graphStatus = document.querySelector("#graph-status"),
    loadoutSelect = document.querySelector("#loadout-select"),
    graphSelect = document.querySelector("#graph-select");
  unsubscribeController = controller.subscribe((s) => {
    apply.disabled = busy || !s.canApply;
    graphStatus.textContent = s.error
      ? `Invalid · ${s.error.code ?? s.error.message}`
      : s.applying
        ? "Applying…"
        : s.dirty
          ? s.staged
            ? "Ready to apply"
            : "Validating…"
          : `Authored revision ${s.revision}`;
  });
  unsubscribeSnapshots = editor.onSnapshots((snapshot) =>
    controller.markDirty(snapshot),
  );
  controller.markDirty(await editor.getState());
  const authored = await controller.apply();
  state.compiled.authored = { ...authored, graphId: "authored_gpu_culling" };
  for (const [name, preset] of Object.entries(renderGraphPresets)) {
    // The explicit AST construction shows the common boundary shared by JSO and FXNode.
    const compiled = await loadGraph(renderer, createGraphAst(preset));
    state.compiled[name] = {
      ...compiled,
      graphId: preset.id,
      revision: preset.revision,
    };
  }
  on(renderer, "renderer-frame", (event) => {
    const expected = state.compiled[state.graph],
      telemetry = event.detail;
    if (
      expected &&
      telemetry.activeCompiledGraph === expected.graphId &&
      sameId(telemetry.activeCompiledId, expected.compiledId) &&
      telemetry.gpuError === false
    ) {
      publish(telemetry);
      if (!busy)
        status(
          `${state.loadout} · ${state.graph} · ${telemetry.draws} draws · ${telemetry.instances} instances`,
        );
    }
  });
  on(
    loadoutSelect,
    "change",
    () => void selectLoadout(loadoutSelect.value, loadoutSelect),
  );
  on(
    graphSelect,
    "change",
    () => void selectGraph(graphSelect.value, graphSelect),
  );
  on(apply, "click", () => {
    const previous = state.graph,
      previousAuthored = state.compiled.authored;
    void transaction(
      "Applying authored graph…",
      async () => {
        const compiled = await controller.apply();
        state.compiled.authored = {
          ...compiled,
          graphId: "authored_gpu_culling",
        };
        state.graph = "authored";
        graphSelect.value = "authored";
        return waitTelemetry(
          (x) =>
            sameId(x.activeCompiledId, compiled.compiledId) &&
            x.activeCompiledGraph === "authored_gpu_culling" &&
            x.activeCompiledRevision === compiled.revision &&
            x.gpuError === false,
        ).catch(() => null);
      },
      async () => {
        state.compiled.authored = previousAuthored;
        state.graph = previous;
        graphSelect.value = previous;
      },
    );
  });
  await editor.whenRendered();
  const initialized = await transaction(
    "Preparing procedural cubes…",
    async () => {
      const targetRevision = (renderer.telemetry?.revision ?? 0) + 1;
      const glb = await loadDemoLoadout("cubes");
      const url = URL.createObjectURL(new Blob([glb], { type: "model/gltf-binary" }));
      try {
        meshHandles.fromImportedScene(await gltfImporter.load(url));
      } finally {
        URL.revokeObjectURL(url);
      }
      await renderer.switchCompiledGraph(authored.compiledId);
      return waitTelemetry(
        (x) =>
          x.revision === targetRevision &&
          x.draws > 0 &&
          x.activeCompiledGraph === "authored_gpu_culling" &&
          x.activeCompiledRevision === authored.revision &&
          x.gpuError === false,
      );
    },
  );
  if (!initialized) throw new Error("Initial demo transaction failed");
  document.documentElement.dataset.yawnReady = "true";
}
const startupError = (error) => {
  if (cleaned) return;
  console.error("Render graph startup failed", error);
  status(`Startup failed · ${error?.code ?? error}`);
  void cleanup();
};
if (document.readyState === "loading")
  document.addEventListener(
    "DOMContentLoaded",
    () => start().catch(startupError),
    { once: true },
  );
else start().catch(startupError);
