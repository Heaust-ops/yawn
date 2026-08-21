# Boot and transport

## Isolation and deployment

`SharedArrayBuffer` requires a secure, cross-origin-isolated page. Serve the document and worker/package assets over HTTPS (localhost is also a secure context) with these response headers:

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Every cross-origin subresource must satisfy COEP (normally CORS or `Cross-Origin-Resource-Policy`). Check `window.crossOriginIsolated === true`, `HTMLCanvasElement.prototype.transferControlToOffscreen`, and `navigator.gpu` before boot. This site's VitePress config applies both headers in dev and preview; production hosting must do the same.

The worker imports `./pkg/yawn_core.js` relative to `core/worker.js`, so deploy the wasm-pack output beside it. The WASM build must use shared memory; initialization otherwise replies `WASM_MEMORY_NOT_SHARED`.

## A robust raw client

```js
const canvas = document.querySelector("canvas");
if (!crossOriginIsolated) throw new Error("cross-origin isolation required");
if (!navigator.gpu) throw new Error("WebGPU required");

const worker = new Worker(new URL("../../core/worker.js", import.meta.url), {
  type: "module",
  name: "yawn-core-raw",
});
let nextRequest = 1;
const pending = new Map();
const profileListeners = new Set();

worker.addEventListener("message", ({ data: message }) => {
  if (message?.type === "profile") {
    for (const listener of profileListeners) listener(message.stats);
    return;
  }
  const operation = pending.get(message?.request);
  if (!operation) return; // late, duplicate, or foreign reply
  pending.delete(message.request);
  if (Object.prototype.hasOwnProperty.call(message, "error")) {
    const error = Object.assign(new Error(message.error), { code: message.error });
    operation.reject(error);
  } else {
    operation.resolve(message.result);
  }
});

function failAll(code) {
  for (const { reject } of pending.values())
    reject(Object.assign(new Error(code), { code }));
  pending.clear();
}
worker.addEventListener("error", () => failAll("WORKER_ERROR"));
worker.addEventListener("messageerror", () => failAll("WORKER_ERROR"));

function request(type, fields = {}, transfer = []) {
  const request = nextRequest++;
  return new Promise((resolve, reject) => {
    pending.set(request, { resolve, reject });
    try {
      worker.postMessage({ type, request, ...fields }, transfer);
    } catch (error) {
      pending.delete(request);
      reject(error);
    }
  });
}

const offscreen = canvas.transferControlToOffscreen();
const { buffer, rows } = await request(
  "init",
  { canvas: offscreen, arenaBytes: 64 * 1024 * 1024 },
  [offscreen],
);
```

Requests are `{ type: string, request: any, ...fields }`. Success is `{ request, result }`; void operations have `result: undefined`. Failure is `{ request, error: string }`. The worker merely echoes `request`, so use unique values. Requests can complete out of order. An unknown message type returns `MESSAGE`; all non-init operations before init return `UNINITIALIZED`.

`init` is one-shot per worker. Its `canvas` **must** be an `OffscreenCanvas` and must appear in the transfer list. `arenaBytes` reaches a Rust `u32`; use an integer from 64 through `2^32 - 64` in practical JS calls. The result contains the shared WASM memory buffer and all initial row descriptors (currently `signals`). Do not transfer the `SharedArrayBuffer`.

## Shutdown

Reject local pending promises, clear listeners/timers, and call `worker.terminate()`. There is no dispose request. Transferred canvases and `ImageBitmap`s cannot be reused by the sender. A fresh worker and canvas are required to restart after termination or failed initialization.
