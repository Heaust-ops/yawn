export {};

type SharedRows = {
  buffer: SharedArrayBuffer;
  descriptor: { offset: number; rows: number; stride: number; format: string };
};
type Box = { id: number; min: number[]; max: number[] };
type Branch = {
  min: number[];
  max: number[];
  boxes?: Box[];
  left?: Branch;
  right?: Branch;
};

let shares: Record<string, SharedRows> = {};
let root: Branch | undefined;
let builtFrame = -1;

function view(name: string) {
  const share = shares[name];
  if (!share) return undefined;
  const length = (share.descriptor.rows * share.descriptor.stride) / 4;
  return share.descriptor.format === "u32"
    ? new Uint32Array(share.buffer, share.descriptor.offset, length)
    : new Float32Array(share.buffer, share.descriptor.offset, length);
}

function merge(boxes: Box[]) {
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (const box of boxes)
    for (let lane = 0; lane < 3; lane++) {
      min[lane] = Math.min(min[lane], box.min[lane]);
      max[lane] = Math.max(max[lane], box.max[lane]);
    }
  return { min, max };
}

function build(boxes: Box[]): Branch | undefined {
  if (!boxes.length) return undefined;
  const bounds = merge(boxes);
  if (boxes.length <= 4) return { ...bounds, boxes };
  const extents = bounds.max.map((value, lane) => value - bounds.min[lane]);
  const axis = extents.indexOf(Math.max(...extents));
  boxes.sort((a, b) => a.min[axis] + a.max[axis] - (b.min[axis] + b.max[axis]));
  const middle = Math.ceil(boxes.length / 2);
  return {
    ...bounds,
    left: build(boxes.slice(0, middle)),
    right: build(boxes.slice(middle)),
  };
}

function rotate(rotor: number[], value: number[]) {
  const [rx, ry, rz, rw] = rotor;
  const [x, y, z] = value;
  const tx = 2 * (ry * z - rz * y);
  const ty = 2 * (rz * x - rx * z);
  const tz = 2 * (rx * y - ry * x);
  return [
    x + rw * tx + ry * tz - rz * ty,
    y + rw * ty + rz * tx - rx * tz,
    z + rw * tz + rx * ty - ry * tx,
  ];
}

function rebuild() {
  const bounds = view("bounds");
  const positions = view("nodePositions");
  const rotors = view("nodeRotors");
  const scales = view("nodeScales");
  const meshes = view("meshInfo");
  const nodes = view("nodes");
  if (!bounds || !positions || !rotors || !scales || !meshes || !nodes)
    return;
  const count = shares.bounds.descriptor.rows;
  const boxes: Box[] = [];
  for (let id = 0; id < count; id++) {
    if (!nodes[id * 4] || !meshes[id * 4 + 2]) continue;
    const offset = id * 8;
    const transform = id * 4;
    const min = [Infinity, Infinity, Infinity];
    const max = [-Infinity, -Infinity, -Infinity];
    const rotor = [0, 1, 2, 3].map((lane) =>
      Number(rotors[transform + lane]),
    );
    for (let corner = 0; corner < 8; corner++) {
      const local = [0, 1, 2].map(
        (lane) =>
          Number(bounds[offset + (corner & (1 << lane) ? 4 : 0) + lane]) *
          Number(scales[transform + lane]),
      );
      const rotated = rotate(rotor, local);
      for (let lane = 0; lane < 3; lane++) {
        const value = rotated[lane] + Number(positions[transform + lane]);
        min[lane] = Math.min(min[lane], value);
        max[lane] = Math.max(max[lane], value);
      }
    }
    if (min.every(Number.isFinite) && max.every(Number.isFinite))
      boxes.push({ id, min, max });
  }
  root = build(boxes);
  builtFrame = Number(view("signals")?.[1] ?? builtFrame + 1);
}

function intersection(
  origin: number[],
  inverse: number[],
  min: number[],
  max: number[],
) {
  let near = -Infinity;
  let far = Infinity;
  for (let lane = 0; lane < 3; lane++) {
    const a = (min[lane] - origin[lane]) * inverse[lane];
    const b = (max[lane] - origin[lane]) * inverse[lane];
    near = Math.max(near, Math.min(a, b));
    far = Math.min(far, Math.max(a, b));
  }
  return far >= Math.max(near, 0) ? Math.max(near, 0) : Infinity;
}

function trace(
  branch: Branch | undefined,
  origin: number[],
  inverse: number[],
  hits: { id: number; distance: number }[],
) {
  if (
    !branch ||
    !Number.isFinite(intersection(origin, inverse, branch.min, branch.max))
  )
    return;
  for (const box of branch.boxes ?? []) {
    const distance = intersection(origin, inverse, box.min, box.max);
    if (Number.isFinite(distance)) hits.push({ id: box.id, distance });
  }
  trace(branch.left, origin, inverse, hits);
  trace(branch.right, origin, inverse, hits);
}

addEventListener("message", ({ data }) => {
  if (data.type === "sync") {
    shares = data.shares;
    rebuild();
    postMessage({ type: "synced", request: data.request });
    return;
  }
  if (data.type === "pick") {
    const frame = Number(view("signals")?.[1] ?? -1);
    if (frame !== builtFrame) rebuild();
    const inverse = data.direction.map((lane: number) => 1 / lane);
    const hits: { id: number; distance: number }[] = [];
    trace(root, data.origin, inverse, hits);
    hits.sort((a, b) => a.distance - b.distance);
    postMessage({ type: "hits", request: data.request, hits });
  }
});
