import type { ColorPickerLayout, Rect } from "./types.js";
import type { Vec2 } from "../core/types.js";
export function layoutColorPicker(anchor: Rect, viewport: Vec2): ColorPickerLayout {
  const padding = 10,
    gap = 8,
    planeSize = 176,
    strip = 18,
    width = 250,
    height = 326;
  let x = anchor.x + anchor.width + 8,
    y = anchor.y;
  if (x + width > viewport.x - 8) x = anchor.x - width - 8;
  if (y + height > viewport.y - 8) y = viewport.y - height - 8;
  x = Math.max(8, x);
  y = Math.max(8, y);
  const fields = (count: number, yy: number) =>
    Array.from({ length: count }, (_, i) => ({
      x: x + padding + (i * (width - padding * 2 + 4)) / count,
      y: yy,
      width: (width - padding * 2 - (count - 1) * 4) / count,
      height: 22,
    }));
  return {
    bounds: { x, y, width, height },
    confirm: { x: x + 6, y: y + 4, width: 24, height: 24 },
    plane: { x: x + padding, y: y + 32, width: planeSize, height: planeSize },
    lightness: { x: x + padding + planeSize + gap, y: y + 32, width: strip, height: planeSize },
    alpha: { x: x + padding + planeSize + gap + strip + gap, y: y + 32, width: strip, height: planeSize },
    rgba: fields(4, y + 220) as unknown as ColorPickerLayout["rgba"],
    hsv: fields(3, y + 250) as unknown as ColorPickerLayout["hsv"],
    hex: { x: x + padding, y: y + 280, width: width - padding * 2, height: 24 },
  };
}
