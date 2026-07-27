import { performance } from "node:perf_hooks";
import { LooseQuadtree } from "@lib/layout/spatial-index.js";

const seed = 7,
  nodes = 5000,
  links = 10000,
  viewport = { x: 1200, y: 800, dpr: 2 };
const nodeIndex = new LooseQuadtree<number>(),
  linkIndex = new LooseQuadtree<number>();
const start = performance.now();
for (let i = 0; i < nodes; i++) {
  const x = (i % 100) * 400,
    y = Math.floor(i / 100) * 300;
  nodeIndex.insert(`n${i}`, i, { minX: x, minY: y - 180, maxX: x + 180, maxY: y });
}
for (let i = 0; i < links; i++) {
  const a = i % nodes,
    row = Math.floor(a / 100),
    b = row * 100 + (((a % 100) + 1 + (i % 7)) % 100),
    ax = (a % 100) * 400 + 180,
    ay = row * 300 - 90,
    bx = (b % 100) * 400,
    by = row * 300 - 90;
  linkIndex.insert(`l${i}`, i, {
    minX: Math.min(ax, bx),
    minY: Math.min(ay, by),
    maxX: Math.max(ax, bx),
    maxY: Math.max(ay, by),
  });
}
const buildMs = performance.now() - start,
  times: number[] = [],
  counts: number[] = [];
for (let i = 0; i < 100; i++) {
  const x = (i * 997) % 38000,
    y = (i * 431) % 14000,
    s = performance.now();
  const nc = nodeIndex.query({ minX: x, minY: y - viewport.y, maxX: x + viewport.x, maxY: y }).length,
    lc = linkIndex.query({ minX: x, minY: y - viewport.y, maxX: x + viewport.x, maxY: y }).length;
  times.push(performance.now() - s);
  counts.push(nc + lc);
}
times.sort((a, b) => a - b);
counts.sort((a, b) => a - b);
const p = (a: number[], q: number) => a[Math.floor((a.length - 1) * q)]!;
const result = {
  seed,
  nodes,
  links,
  viewport,
  buildMs: +buildMs.toFixed(3),
  candidateP50: p(counts, 0.5),
  candidateP95: p(counts, 0.95),
  queryMsP50: +p(times, 0.5).toFixed(3),
  queryMsP95: +p(times, 0.95).toFixed(3),
  cullingP95: +(1 - p(counts, 0.95) / (nodes + links)).toFixed(4),
};
console.log(JSON.stringify(result));
if (
  process.argv.includes("--check") &&
  (result.seed !== 7 ||
    result.nodes !== 5000 ||
    result.links !== 10000 ||
    result.viewport.x !== 1200 ||
    result.viewport.y !== 800 ||
    result.viewport.dpr !== 2 ||
    times.length !== 100 ||
    result.candidateP50 !== 72 ||
    result.candidateP95 !== 75 ||
    result.cullingP95 < 0.9)
)
  throw new Error(`Phase 7 deterministic workload baseline missed: ${JSON.stringify(result)}`);
