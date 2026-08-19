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
      const { buffer, descriptor } = message;
      if (!(buffer instanceof SharedArrayBuffer) || descriptor?.format !== "u32" ||
          descriptor.stride !== 16 || descriptor.offset % 64 || packet.byteLength > descriptor.rows * descriptor.stride)
        throw new Error("GLTF_STORAGE_INVALID");
      new Uint8Array(buffer, descriptor.offset, packet.byteLength).set(packet);
      downloads.delete(request);
      postMessage({ type: "ready", request, byteLength: packet.byteLength });
    }
  } catch (error) {
    downloads.delete(request);
    postMessage({ type: "error", request, error: error?.message || "GLTF_IMPORT_FAILED" });
  }
});
