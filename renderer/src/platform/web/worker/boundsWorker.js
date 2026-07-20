// Sole writer for the LocalBoundsData region in shared Wasm memory.
export function attachBounds() {}

let words;
let capacity;
let headerWords;

onmessage = ({ data }) => {
  if (data.type === "init") {
    const d = data.descriptor;
    if (d[0] !== 0x424e4453 || d[1] !== 1 || d[5] !== 12 || d[6] !== 1) {
      words = undefined;
      postMessage({ type: "error", code: "invalid-descriptor", message: "unsupported bounds SAB descriptor" });
      return;
    }
    words = new Uint32Array(data.memory.buffer);
    capacity = d[3];
    headerWords = d[4];
    return;
  }
  if (data.type !== "job" || !words) return;
  if (data.slot >= capacity) {
    postMessage({ type: "error", message: `bounds slot ${data.slot} exceeds capacity ${capacity}` });
    return;
  }
  const positions = new Float32Array(data.positions);
  let state = 1;
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  if (positions.length === 0) state = 2;
  else if (positions.length % 3 !== 0) state = 3;
  else for (let i = 0; i < positions.length; i++) {
    const value = positions[i], axis = i % 3;
    if (!Number.isFinite(value)) { state = 3; break; }
    min[axis] = Math.min(min[axis], value); max[axis] = Math.max(max[axis], value);
  }
  if (state !== 1) min.fill(0), max.fill(0);
  const at = (column) => (data.pointer >>> 2) + headerWords + column * capacity + data.slot;
  Atomics.add(words, at(0), 1); // odd: publication in progress
  const values = [data.generation, data.contentVersion, state, data.jobId, data.snapshotId,
    ...min.map(floatBits), ...max.map(floatBits)];
  values.forEach((value, index) => Atomics.store(words, at(index + 1), value));
  Atomics.add(words, at(0), 1); // even: release publication
  postMessage({ type: "complete", jobId: data.jobId });
};

function floatBits(value) {
  const buffer = new ArrayBuffer(4);
  new Float32Array(buffer)[0] = value;
  return new Uint32Array(buffer)[0];
}
