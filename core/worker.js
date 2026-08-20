import initWasm, { Core } from "./pkg/yawn_core.js";

let core;
let profileTimer;

const fail = code => { throw new Error(code); };

addEventListener("message", async ({ data: message }) => {
  try {
    let result;
    if (message?.type === "init") {
      if (core || !(message.canvas instanceof OffscreenCanvas)) fail("INIT");
      const wasm = await initWasm();
      core = new Core(message.arenaBytes);
      await core.initialize(message.canvas);
      const buffer = wasm.memory.buffer;
      if (!(buffer instanceof SharedArrayBuffer)) fail("WASM_MEMORY_NOT_SHARED");
      result = { buffer, rows: JSON.parse(core.rows()) };
    } else {
      if (!core) fail("UNINITIALIZED");
      switch (message?.type) {
        case "create-rows":
          result = JSON.parse(core.create_rows(message.name, message.rows, message.stride, message.format));
          break;
        case "create-rows-batch":
          result = JSON.parse(core.create_rows_batch(JSON.stringify(message.rows)));
          break;
        case "delete-rows":
          core.delete_rows(message.name);
          break;
        case "allocate-object":
          result = JSON.parse(core.allocate_object(message.name));
          break;
        case "delete-object":
          core.delete_object(message.name, message.id);
          break;
        case "compile-graph":
          result = core.compile_graph(message.serialized);
          break;
        case "switch-loadout":
          core.switch_loadout(message.id);
          break;
        case "upload-texture":
          core.upload_texture(message.name, message.image);
          break;
        case "delete-texture":
          core.delete_texture(message.name);
          break;
        case "play":
          core.play();
          break;
        case "pause":
          core.pause();
          break;
        case "set-fps":
          core.set_fps(message.fps);
          break;
        case "set-profiler":
          result = core.set_profiler(Boolean(message.enabled));
          clearInterval(profileTimer);
          profileTimer = result && message.enabled ? setInterval(() => {
            const stats = core.take_profile();
            if (stats) postMessage({ type: "profile", stats: JSON.parse(stats) });
          }, 250) : undefined;
          break;
        default:
          fail("MESSAGE");
      }
    }
    postMessage({ request: message.request, result });
  } catch (error) {
    postMessage({ request: message?.request, error: error?.message ?? "CORE_ERROR" });
  }
});
