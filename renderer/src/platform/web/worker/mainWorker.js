// Generic worker that imports the app's WASM module relative to the generated pkg folder.
// Works for any application because the relative depth from this file to pkg is stable.
import initWasm, { worker_entrypoint, worker_maintain_renderer, worker_register_scene_payload, worker_report_scene_error, worker_report_pick } from "/level-editor/pkg/level_editor.js";

export function attachMain() {}

let initializationStarted = false;
let wasmReady = false;
const wasmMessages = [];
let boundsWorker;
let bvhWorker;
let sharedMemory;
let boundsDescriptorPointer = 0;
let latestBoundsInit;
let latestBvhSnapshot;
const pendingBoundsJobs = new Map();
const pendingPicks = new Map();

export function takeStartupCanvas() {
  // wasm-bindgen/Vite can instantiate this source module twice in the same
  // worker bundle. The worker global is the single startup ownership boundary.
  const canvas = globalThis.__rendererStartupCanvas;
  globalThis.__rendererStartupCanvas = undefined;
  return canvas;
}

function reportPick(data) {
  try {
    worker_report_pick(data.requestId, data.status, data.slot ?? 0, data.generation ?? 0, data.snapshotId, data.publicationVersion ?? 0);
  } catch (error) {
    console.error("failed to deliver BVH result to WASM", { requestId: data.requestId, message: String(error.message || error) });
  }
}

function createBoundsWorker() {
  const worker = new Worker(new URL("./boundsWorker.js", import.meta.url), { type: "module", name: "geometry-bounds" });
  const failed = (event) => {
    event?.preventDefault?.();
    console.error("bounds worker failed", { type: event?.type, message: event?.message || "unreadable worker message" });
    if (boundsWorker !== worker) return;
    worker.terminate();
    boundsWorker = undefined;
    if (latestBoundsInit) {
      boundsWorker = createBoundsWorker();
      boundsWorker.postMessage(latestBoundsInit);
      for (const job of pendingBoundsJobs.values()) postBoundsJob(boundsWorker, job);
    }
  };
  worker.onerror = failed;
  worker.onmessageerror = failed;
  worker.onmessage = ({ data }) => {
    if (data.type === "complete") {
      pendingBoundsJobs.delete(data.jobId);
      if (wasmReady) worker_maintain_renderer();
      return;
    }
    if (data.type === "error") console.error("bounds worker reported an error", data);
  };
  return worker;
}

function postBoundsJob(worker, job) {
  const positions = job.positions.slice(0);
  worker.postMessage({ ...job, positions }, [positions]);
}

function createBvhWorker() {
  const worker = new Worker(new URL("./bvhWorker.js", import.meta.url), { type: "module", name: "scene-bvh" });
  const failed = (event) => {
    event?.preventDefault?.();
    console.error("BVH worker failed", { type: event?.type, message: event?.message || "unreadable worker message" });
    if (bvhWorker !== worker) return;
    worker.terminate();
    bvhWorker = undefined;
    for (const pick of pendingPicks.values()) {
      reportPick({ requestId: pick.requestId, status: "error", snapshotId: pick.spatialSnapshotId });
    }
    pendingPicks.clear();
    if (latestBvhSnapshot) {
      bvhWorker = createBvhWorker();
      bvhWorker.postMessage(latestBvhSnapshot);
    }
  };
  worker.onerror = failed;
  worker.onmessageerror = failed;
  worker.onmessage = ({ data }) => {
    if (data.type === "error") {
      console.error("BVH worker reported an error", data);
      return;
    }
    if (data.type !== "pick-result") return;
    pendingPicks.delete(data.requestId);
    reportPick(data);
    if (wasmReady) worker_maintain_renderer();
    if (data.publicationVersion) worker.postMessage({ type: "ack", publicationVersion: data.publicationVersion });
  };
  return worker;
}

export function routeRendererMessage(data, transfer = []) {
  if (data?.type === "bounds-init") {
    boundsWorker ??= createBoundsWorker();
    const descriptor = Array.from(data.descriptor);
    boundsDescriptorPointer = descriptor[2];
    latestBoundsInit = { type: "init", descriptor, memory: data.memory ?? sharedMemory };
    boundsWorker.postMessage(latestBoundsInit);
    return true;
  }
  if (data?.type === "bounds-job") {
    if (!boundsWorker) { console.error("bounds job received before descriptor"); return true; }
    const { positions, ...job } = data;
    job.type = "job";
    job.pointer = boundsDescriptorPointer;
    const cached = { ...job, positions: positions.slice(0) };
    pendingBoundsJobs.set(job.jobId, cached);
    boundsWorker.postMessage({ ...job, positions }, transfer.length ? transfer : [positions]);
    return true;
  }
  if (data?.type === "spatial-snapshot" || data?.type === "pick-request") {
    bvhWorker ??= createBvhWorker();
    const message = data.type === "spatial-snapshot" ? { ...data, type: "snapshot" } : { ...data, type: "pick" };
    if (message.type === "snapshot") latestBvhSnapshot = message;
    else pendingPicks.set(message.requestId, message);
    bvhWorker.postMessage(message);
    return true;
  }
  return false;
}

onmessage = async (event) => {
  if (event.data?.type === "renderer-wake") {
    if (wasmReady) {
      try { worker_maintain_renderer(); }
      catch (error) { postMessage({ type: "renderer-startup-error", message: String(error.message || error) }); }
    } else wasmMessages.push(() => worker_maintain_renderer());
    return;
  }
  if (event.data?.type === "io-port") {
    const port = event.ports[0];
    port.onmessage = ({ data }) => {
      wasmMessages.push(() => {
        try {
          if (data.type === "payload") {
            worker_register_scene_payload(data.loadId, new Uint8Array(data.buffer));
            worker_maintain_renderer();
          } else if (data.type === "error") {
            worker_report_scene_error(data.loadId, data.error?.message ?? data.message);
            worker_maintain_renderer();
          }
          else console.error("unknown file I/O message", data);
        } catch (error) {
          console.error("failed to deliver file I/O message to WASM", { loadId: data.loadId, message: String(error.message || error) });
        } finally {
          port.postMessage({ type: "ack", loadId: data.loadId });
        }
      });
      if (wasmReady) wasmMessages.shift()();
    };
    port.start();
    return;
  }
  if (routeRendererMessage(event.data, event.ports)) return;
  console.log("worker received message", event);
  if (initializationStarted) return;
  const startup = event.data;
  if (startup?.type !== "renderer-start" || startup.protocolVersion !== 1) {
    postMessage({ type: "renderer-startup-error", message: "invalid renderer startup envelope" });
    return;
  }
  initializationStarted = true;
  const wasmModule = startup.module;
  const workerId = startup.workerId;
  const memory = startup.memory;
  sharedMemory = memory;
  const entryPtr = startup.entryPtr;
  globalThis.__rendererStartupCanvas = startup.canvas;
  if (!(globalThis.__rendererStartupCanvas instanceof OffscreenCanvas)) {
    postMessage({ type: "renderer-startup-error", message: "startup envelope omitted OffscreenCanvas" });
    return;
  }

  console.log(
    "worker: initializing with WASM module",
    wasmModule,
    "id:",
    workerId,
  );

  // Initialize WASM with the shared module and memory forwarded from the main thread.
  try {
    await initWasm({ module_or_path: wasmModule, memory });

    // Calls into WASM are only safe after the app entrypoint has completed.
    worker_entrypoint(entryPtr);
    wasmReady = true;
    while (wasmMessages.length) wasmMessages.shift()();
  } catch (error) {
    const message = String(error.message || error);
    console.error("worker WASM initialization failed", { message });
    while (wasmMessages.length) wasmMessages.shift()();
    try { postMessage({ type: "renderer-startup-error", message }); }
    catch (postError) { console.error("failed to report renderer startup failure", { message: String(postError.message || postError) }); }
  }
};
