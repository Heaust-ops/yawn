/** Create the worker bridge expected by YawnCore for one OffscreenCanvas. */
export function createWorkerTransport(canvas) {
  if (!(canvas instanceof HTMLCanvasElement)) {
    throw new TypeError("canvas must be an HTMLCanvasElement");
  }
  const dimensions = () => {
    const dpr = devicePixelRatio;
    return {
      dpr,
      width: Math.max(1, canvas.clientWidth),
      height: Math.max(1, canvas.clientHeight),
    };
  };
  const initial = dimensions();
  canvas.width = Math.round(initial.width * initial.dpr);
  canvas.height = Math.round(initial.height * initial.dpr);
  const worker = new Worker(
    new URL(
      "../../renderer/src/platform/web/worker/mainWorker.js",
      import.meta.url,
    ),
    { type: "module", name: "yawn-renderer" },
  );
  const resize = () => {
    const { dpr, width, height } = dimensions();
    worker.postMessage({
      type: "window-event",
      kind: 0,
      values: new Float64Array([width, height, dpr]),
    });
  };
  const abort = new AbortController();
  addEventListener("resize", resize, { signal: abort.signal });
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ type: "init", canvas: offscreen }, [offscreen]);
  return {
    worker,
    free() {
      abort.abort();
    },
  };
}
