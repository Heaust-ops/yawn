import { YawnCore } from "@yawn/core";

/** Connect from any worker using a MessagePort with the Worker-like transport API. */
export async function connectFromWorker(port, options = {}) {
  const core = new YawnCore({
    worker: port,
    memory: options.memory,
    ringPtr: options.ringPtr,
    free: options.free,
  });
  await core.ready;
  return core;
}
