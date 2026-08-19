import { writeSharedUpload } from "./shared-upload.js";
import { gltfToRenderDataPacket } from "./gltf.js";

const downloads = new Map();

addEventListener("message", async ({ data: message }) => {
  const request = message?.request;
  try {
    if (message?.type === "load") {
      const response = await fetch(message.url);
      if (!response.ok) throw new Error(`HTTP_${response.status}`);
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (!bytes.byteLength) throw new Error("GLTF_EMPTY");
      const packet = await gltfToRenderDataPacket(bytes, message.url);
      downloads.set(request, packet);
      postMessage({ type: "allocate", request, byteLength: packet.byteLength });
      return;
    }
    if (message?.type === "storage") {
      const packet = downloads.get(request);
      if (!packet) throw new Error("GLTF_REQUEST_UNKNOWN");
      writeSharedUpload(message.buffer, message.descriptor, packet);
      downloads.delete(request);
      postMessage({ type: "ready", request, byteLength: packet.byteLength });
    }
  } catch (error) {
    downloads.delete(request);
    postMessage({ type: "error", request, code: error?.message || "GLTF_IMPORT_FAILED" });
  }
});
