// The render worker owns its only WASM and WebGPU runtime.
import initWasm, {
  clear_payloads,
  discard_payload,
  stage_payload,
  worker_main,
  worker_memory,
  worker_window_event,
} from "/renderer/pkg/renderer.js";

function listenerReady() {
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
  const { canvas } = message;

  // The renderer worker exclusively owns the one WASM instance. Other threads
  // receive only its shared memory and mutate the published SAB layouts.
  try {
    api = await initWasm();
  } catch (error) {
    fatal("WORKER_INIT_FAILED", error?.stack || String(error));
    return;
  }
  state = "waiting-listener";
  pending.push({ type: "canvas", canvas });
  try {
    const ringPtr = worker_main();
    postMessage({ type: "bootstrap", memory: worker_memory(), ringPtr });
    setTimeout(listenerReady, 0);
  } catch (error) {
    fatal("WORKER_ENTRY_FAILED", error?.stack || String(error));
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
  } else if (message?.type === "window-event") {
    worker_window_event(message.kind, message.values);
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
