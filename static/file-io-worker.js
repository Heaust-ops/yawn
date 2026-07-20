let renderPort;
let active;
let activeRequest;
let inFlight = false;
let queued;
let latestLoadId = 0;
const MAX_SOURCE_BYTES = 384 * 1024 * 1024;

onmessage = ({ data, ports }) => {
  if (data.type === "connect") {
    renderPort?.close();
    if (inFlight && activeRequest?.loadId === latestLoadId) queued = activeRequest;
    inFlight = false;
    renderPort = ports[0];
    renderPort.onmessage = ({ data: reply }) => {
      if (reply.type === "ack") {
        inFlight = false;
        pump();
      }
    };
    renderPort.onmessageerror = () => disconnect("renderer port received an unreadable message");
    renderPort.start();
    pump();
  } else if (data.type === "load") {
    latestLoadId = data.loadId;
    active?.abort();
    queued = data;
    pump();
  }
};

function disconnect(message) {
  console.error("file I/O worker disconnected", { code: "io-port-failed", message });
  renderPort?.close();
  renderPort = undefined;
  if (activeRequest?.loadId === latestLoadId) queued = activeRequest;
  inFlight = false;
}

function pump() {
  if (!renderPort || inFlight || !queued) return;
  const request = queued;
  queued = undefined;
  read(request);
}

async function read(request) {
  const { loadId, url, handle, file } = request;
  const controller = new AbortController();
  active = controller;
  try {
    let source = file;
    if (handle) {
      if ((await handle.queryPermission({ mode: "read" })) !== "granted") {
        throw new Error("permission-required: select the file again to grant read access");
      }
      source = await handle.getFile();
    }
    if (source && source.size > MAX_SOURCE_BYTES) throw new RangeError("scene exceeds source byte budget");
    let buffer;
    if (source) buffer = await source.arrayBuffer();
    else {
      if (url !== "/themanor.glb" && url !== "/sponza.glb") throw new Error("unsupported bundled URL");
      const response = await fetch(url, { signal: controller.signal });
      if (!response.ok) throw new Error(`HTTP ${response.status} reading ${url}`);
      const declared = Number(response.headers.get("content-length"));
      if (Number.isFinite(declared) && declared > MAX_SOURCE_BYTES) throw new RangeError("scene exceeds source byte budget");
      if (!response.body) throw new Error("response body is unavailable");
      const reader = response.body.getReader();
      const chunks = [];
      let total = 0;
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        total = total + value.byteLength;
        if (!Number.isSafeInteger(total) || total > MAX_SOURCE_BYTES) {
          await reader.cancel("scene exceeds source byte budget");
          throw new RangeError("scene exceeds source byte budget");
        }
        chunks.push(value);
      }
      const combined = new Uint8Array(total);
      let offset = 0;
      for (const chunk of chunks) { combined.set(chunk, offset); offset += chunk.byteLength; }
      buffer = combined.buffer;
    }
    if (buffer.byteLength > MAX_SOURCE_BYTES) throw new RangeError("scene exceeds source byte budget");
    if (loadId !== latestLoadId) return;
    send(request, { type: "payload", loadId, buffer }, [buffer]);
  } catch (error) {
    if (error.name === "AbortError") return;
    if (loadId !== latestLoadId) return;
    const details = {
      code: error.name || "io-error",
      message: String(error.message || error),
    };
    send(request, { type: "error", loadId, message: details.message, error: details });
  }
}

function send(request, message, transfer = []) {
  if (request.loadId !== latestLoadId) return;
  if (!renderPort) { queued = request; return; }
  try {
    activeRequest = request;
    inFlight = true;
    renderPort.postMessage(message, transfer);
  } catch (error) {
    queued = request;
    disconnect(String(error.message || error));
  }
}
