import { writeSharedUpload } from "./shared-upload.js";

const downloads = new Map();

addEventListener("message", async ({ data: message }) => {
  const request = message?.request;
  try {
    if (message?.type === "load") {
      const response = await fetch(message.url);
      if (!response.ok) throw new Error(`HTTP_${response.status}`);
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (!bytes.byteLength) throw new Error("GLTF_EMPTY");
      downloads.set(request, bytes);
      postMessage({ type: "allocate", request, byteLength: bytes.byteLength });
      return;
    }
    if (message?.type === "storage") {
      const bytes = downloads.get(request);
      if (!bytes) throw new Error("GLTF_REQUEST_UNKNOWN");
      writeSharedUpload(message.buffer, message.descriptor, bytes);
      downloads.delete(request);
      postMessage({ type: "ready", request, byteLength: bytes.byteLength });
    }
  } catch (error) {
    downloads.delete(request);
    postMessage({ type: "error", request, code: error?.message || "GLTF_IMPORT_FAILED" });
  }
});
