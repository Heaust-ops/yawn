const MIN_DISTANCE = 0.1;
const MAX_PITCH = Math.PI / 2 - 0.01;

// Browser controls map straight onto the packed camera SOA row without messages.
export function installCameraRenderDataControls(core, canvas) {
  const camera = core.array("camera.state");
  const abort = new AbortController();
  const options = { signal: abort.signal };
  const write = (state) => camera.write(0, state);

  canvas.addEventListener(
    "pointerdown",
    (event) => {
      if (event.pointerType === "mouse" && (event.button === 1 || event.button === 2)) {
        event.preventDefault();
        canvas.setPointerCapture(event.pointerId);
      }
    },
    options,
  );
  canvas.addEventListener(
    "pointermove",
    (event) => {
      if (
        event.pointerType !== "mouse" ||
        !canvas.hasPointerCapture(event.pointerId) ||
        (event.buttons & 6) === 0
      ) {
        return;
      }
      event.preventDefault();
      const state = camera.read(0);
      const offset = [state[0] - state[4], state[1] - state[5], state[2] - state[6]];
      const distance = Math.hypot(...offset);
      if ((event.buttons & 4) !== 0) {
        const yaw = Math.atan2(offset[0], offset[2]) + event.movementX * 0.005;
        const pitch = Math.max(
          -MAX_PITCH,
          Math.min(
            MAX_PITCH,
            Math.asin(offset[1] / distance) + event.movementY * 0.005,
          ),
        );
        const horizontal = Math.cos(pitch) * distance;
        state[0] = state[4] + Math.sin(yaw) * horizontal;
        state[1] = state[5] + Math.sin(pitch) * distance;
        state[2] = state[6] + Math.cos(yaw) * horizontal;
      } else {
        const forward = [
          (state[4] - state[0]) / distance,
          (state[5] - state[1]) / distance,
          (state[6] - state[2]) / distance,
        ];
        const right = [
          forward[1] * state[10] - forward[2] * state[9],
          forward[2] * state[8] - forward[0] * state[10],
          forward[0] * state[9] - forward[1] * state[8],
        ];
        const rightLength = Math.hypot(...right);
        right.forEach((value, index) => (right[index] = value / rightLength));
        const up = [
          right[1] * forward[2] - right[2] * forward[1],
          right[2] * forward[0] - right[0] * forward[2],
          right[0] * forward[1] - right[1] * forward[0],
        ];
        const units =
          (2 * distance * Math.tan(state[12] * 0.5)) /
          Math.max(1, canvas.clientHeight);
        const translation = right.map(
          (value, index) =>
            -value * event.movementX * units + up[index] * event.movementY * units,
        );
        for (let index = 0; index < 3; index++) {
          state[index] += translation[index];
          state[index + 4] += translation[index];
        }
      }
      write(state);
    },
    options,
  );
  for (const type of ["pointerup", "pointercancel"]) {
    canvas.addEventListener(
      type,
      (event) => {
        if (canvas.hasPointerCapture(event.pointerId)) {
          canvas.releasePointerCapture(event.pointerId);
        }
      },
      options,
    );
  }
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const delta =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? event.deltaY * 16
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? event.deltaY * canvas.clientHeight
            : event.deltaY;
      if (!Number.isFinite(delta) || delta === 0) return;
      const state = camera.read(0);
      const offset = [state[0] - state[4], state[1] - state[5], state[2] - state[6]];
      const distance = Math.hypot(...offset);
      const nextDistance = Math.max(
        MIN_DISTANCE,
        Math.min(state[15] * 0.95, distance * Math.exp(0.002 * delta)),
      );
      for (let index = 0; index < 3; index++) {
        state[index] = state[index + 4] + offset[index] * (nextDistance / distance);
      }
      write(state);
    },
    { ...options, passive: false },
  );
  canvas.addEventListener("contextmenu", (event) => event.preventDefault(), options);

  return () => abort.abort();
}
