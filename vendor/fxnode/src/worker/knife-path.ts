import type { LinkId, Vec2 } from "../core/types.js";
import { worldToView } from "../layout/geometry.js";
import type { LayoutSnapshot } from "../layout/types.js";

export const MAX_KNIFE_POINTS = 256;
export function appendKnifePoint(points: readonly Vec2[], point: Vec2, minimumDistance = 2): readonly Vec2[] {
  const last = points.at(-1);
  if (last && Math.hypot(last.x - point.x, last.y - point.y) < minimumDistance) return points;
  return points.length < MAX_KNIFE_POINTS ? [...points, point] : [...points.slice(1), point];
}
const orient = (a: Vec2, b: Vec2, c: Vec2) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
const between = (a: number, b: number, x: number) => x >= Math.min(a, b) - 1e-7 && x <= Math.max(a, b) + 1e-7;
export function segmentsIntersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2): boolean {
  const abC = orient(a, b, c),
    abD = orient(a, b, d),
    cdA = orient(c, d, a),
    cdB = orient(c, d, b);
  if (((abC > 0 && abD < 0) || (abC < 0 && abD > 0)) && ((cdA > 0 && cdB < 0) || (cdA < 0 && cdB > 0))) return true;
  return (
    (Math.abs(abC) < 1e-7 && between(a.x, b.x, c.x) && between(a.y, b.y, c.y)) ||
    (Math.abs(abD) < 1e-7 && between(a.x, b.x, d.x) && between(a.y, b.y, d.y)) ||
    (Math.abs(cdA) < 1e-7 && between(c.x, d.x, a.x) && between(c.y, d.y, a.y)) ||
    (Math.abs(cdB) < 1e-7 && between(c.x, d.x, b.x) && between(c.y, d.y, b.y))
  );
}
export function crossedLinks(layout: LayoutSnapshot, path: readonly Vec2[], includeMuted = false): Set<LinkId> {
  const result = new Set<LinkId>();
  if (path.length < 2) return result;
  const planned = layout as LayoutSnapshot & { candidateLinkIds?: readonly LinkId[] };
  for (const id of planned.candidateLinkIds ?? layout.links.keys()) {
    const link = layout.links.get(id);
    if (!link?.visible || (!includeMuted && link.muted)) continue;
    const samples = link.points.map((p) => worldToView(p, layout.transform));
    outer: for (let i = 1; i < path.length; i++)
      for (let j = 1; j < samples.length; j++)
        if (segmentsIntersect(path[i - 1]!, path[i]!, samples[j - 1]!, samples[j]!)) {
          result.add(id);
          break outer;
        }
  }
  return result;
}
