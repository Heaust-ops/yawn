import wbg_init, { main } from "../level-editor/pkg/level_editor.js";
import { Renderer } from "./renderer-api.js";

const start = async () => {
  const wasm = await wbg_init();
  const renderer = new Renderer(wasm.memory);
  globalThis.rendererAbiReady = (message) => renderer.attach(message.descriptor);
  const connectRendererWorker = globalThis.rendererWorkerReady;
  globalThis.rendererWorkerReady = (worker) => {
    connectRendererWorker?.(worker);
    worker.addEventListener("message", ({ data }) => {
      if (data?.type !== "pick-status") return;
      const text = data.status === "hit" ? ` Selected ${data.slot}:${data.generation} (AABB candidate)` : data.status === "stale" ? " Pick stale; retry" : " No AABB hit";
      document.querySelector("#pick-status").textContent = text;
    });
  };
  globalThis.renderer = renderer;
  main();
  const status = document.querySelector("#load-status");
  const load = async (source) => {
    status.textContent = " Loading…";
    try { await renderer.loadScene(source); status.textContent = " Loaded"; }
    catch (error) { status.textContent = ` ${error.message}`; console.error(error); }
  };
  document.querySelector("#scene").addEventListener("change", (event) => {
    load(event.target.value);
  });
  const input = document.querySelector("#file-input");
  input.addEventListener("change", () => input.files[0] && load(input.files[0]));
  document.querySelector("#load-file").addEventListener("click", async () => {
    if (!("showOpenFilePicker" in window)) { input.click(); return; }
    try {
      const [handle] = await showOpenFilePicker({ multiple: false, types: [{ description: "Binary glTF", accept: { "model/gltf-binary": [".glb"] } }] });
      load(handle);
    } catch (error) { if (error.name !== "AbortError") status.textContent = ` ${error.message}`; }
  });
};

// Wait for DOM to be ready before starting
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", start);
} else {
  start();
}
