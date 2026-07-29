// Generic worker that imports the app's WASM module relative to the generated pkg folder.
// Works for any application because the relative depth from this file to pkg is stable.
import initWasm, { clear_payloads, discard_payload, stage_payload, worker_entrypoint } from "/level-editor/pkg/level_editor.js";

export function listenerReady() {
  if (state !== "waiting-listener") return;
  state = "replaying";
  for (const queued of pending.splice(0)) route(queued);
  state = "ready";
}

let api;
let state = "uninitialized";
const pending = [];

// This listener is never replaced: canvas and payload transfers that race WASM
// initialization remain ordered and are replayed after init.
addEventListener("message", async (event) => {
  const message = event.data;
  if (message?.type !== "init") {
    if (state !== "ready") pending.push(message);
    else route(message);
    return;
  }
  if (state !== "uninitialized") return;
  state = "initializing";
  const { wasmModule, workerId, memory, entryPtr } = message;

  console.log(
    "worker: initializing with WASM module",
    wasmModule,
    "id:",
    workerId,
  );

  // Initialize WASM with the shared module and memory forwarded from the main thread.
  try {
    api = await initWasm({ module_or_path: wasmModule, memory });
    state = "waiting-listener";
    worker_entrypoint(entryPtr);
  } catch (error) {
    fatal("WORKER_INIT_FAILED", String(error));
  }
});

function route(message) {
  if (message?.type === "canvas") {
    dispatchEvent(new MessageEvent("renderer-canvas", { data: message.canvas }));
  } else if (message?.type === "payload") {
    stage_payload(message.id, new Uint8Array(message.buffer));
    postMessage({ type: "payload-ready", id: message.id });
  } else if (message?.type === "payload-release") {
    discard_payload(message.id);
  }
}

function fatal(code, message) {
  state = "failed";
  pending.length = 0;
  try { clear_payloads?.(); } catch { /* best effort during a fatal failure */ }
  postMessage({type:"fatal",code,message});
}
addEventListener("error", event => fatal("WORKER_RUNTIME_ERROR", event.error?.stack || `${event.message} (${event.filename}:${event.lineno}:${event.colno})`));
addEventListener("unhandledrejection", event => fatal("WORKER_UNHANDLED_REJECTION",String(event.reason)));
