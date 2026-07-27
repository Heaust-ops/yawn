import type { Vec2 } from "../core/types.js";
import type { Rect, ViewTransform } from "./types.js";

export const worldToView = (p: Vec2, t: ViewTransform): Vec2 => ({
  x: (p.x - t.center.x) * t.zoom + t.viewport.x / 2,
  y: (t.center.y - p.y) * t.zoom + t.viewport.y / 2,
});
export const viewToWorld = (p: Vec2, t: ViewTransform): Vec2 => ({
  x: (p.x - t.viewport.x / 2) / t.zoom + t.center.x,
  y: t.center.y - (p.y - t.viewport.y / 2) / t.zoom,
});
export const viewToDevice = (p: Vec2, t: ViewTransform): Vec2 => ({ x: p.x * t.dpr, y: p.y * t.dpr });
export const deviceToView = (p: Vec2, t: ViewTransform): Vec2 => ({ x: p.x / t.dpr, y: p.y / t.dpr });
export const intersects = (a: Rect, b: Rect, overscan = 0): boolean =>
  a.x + a.width >= b.x - overscan &&
  a.x <= b.x + b.width + overscan &&
  a.y - a.height <= b.y + overscan &&
  a.y >= b.y - b.height - overscan;
export function bounds(points: readonly Vec2[]): Rect {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const minX = Math.min(...xs),
    maxX = Math.max(...xs),
    minY = Math.min(...ys),
    maxY = Math.max(...ys);
  return { x: minX, y: maxY, width: maxX - minX, height: maxY - minY };
}
