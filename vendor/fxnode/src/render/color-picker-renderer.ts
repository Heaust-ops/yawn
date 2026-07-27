import type { ColorPickerLayout, Rect } from "../layout/types.js";
import { mapOklchToSrgb, maxSrgbChroma, type Oklch, type Rgba } from "../color/oklab.js";

const cache = new Map<string, ImageData>();
export interface DevicePaintTarget {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

function wheel(size: number, l: number): ImageData {
  const bin = Math.round(l * 32),
    key = `${size}:${bin}`,
    known = cache.get(key);
  if (known) return known;
  const image = new ImageData(size, size),
    radius = size / 2,
    lightness = bin / 32;
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      const dx = x + 0.5 - radius,
        dy = radius - y - 0.5,
        r = Math.hypot(dx, dy) / radius,
        index = (y * size + x) * 4;
      if (r > 1) continue;
      const h = Math.atan2(dy, dx),
        rgb = mapOklchToSrgb({ l: lightness, c: r * maxSrgbChroma(lightness, h), h });
      image.data[index] = rgb[0] * 255;
      image.data[index + 1] = rgb[1] * 255;
      image.data[index + 2] = rgb[2] * 255;
      image.data[index + 3] = 255;
    }
  cache.set(key, image);
  while (cache.size > 66) cache.delete(cache.keys().next().value!);
  return image;
}

export function paintOklchWheel(
  context: OffscreenCanvasRenderingContext2D,
  bounds: { plane: Rect; lightness: Rect },
  model: Oklch,
  dpr = 1,
  planeLightness = model.l,
  target: DevicePaintTarget = { x: 0, y: 0, width: Number.MAX_SAFE_INTEGER, height: Number.MAX_SAFE_INTEGER },
): void {
  const size = Math.max(1, Math.round(bounds.plane.width * dpr)),
    image = wheel(size, planeLightness),
    destinationX = target.x + Math.round(bounds.plane.x * dpr),
    destinationY = target.y + Math.round(bounds.plane.y * dpr),
    targetRight = target.x + target.width,
    targetBottom = target.y + target.height;
  // putImageData ignores transforms and clipping. Copy only each opaque wheel
  // scanline so transparent corners preserve the picker background and no row
  // can write into an adjacent atlas slot.
  for (let y = 0; y < size; y++) {
    const deviceY = destinationY + y;
    if (deviceY < target.y || deviceY >= targetBottom) continue;
    let first = 0,
      last = size - 1;
    while (first < size && image.data[(y * size + first) * 4 + 3] === 0) first++;
    while (last >= first && image.data[(y * size + last) * 4 + 3] === 0) last--;
    first = Math.max(first, target.x - destinationX);
    last = Math.min(last, targetRight - destinationX - 1);
    if (first <= last) context.putImageData(image, destinationX, destinationY, first, y, last - first + 1, 1);
  }
  const light = context.createLinearGradient(0, bounds.lightness.y, 0, bounds.lightness.y + bounds.lightness.height);
  for (let i = 0; i <= 8; i++) {
    const l = 1 - i / 8,
      rgb = mapOklchToSrgb({ l, c: model.c, h: model.h });
    light.addColorStop(i / 8, `rgb(${rgb[0] * 255} ${rgb[1] * 255} ${rgb[2] * 255})`);
  }
  context.fillStyle = light;
  context.fillRect(bounds.lightness.x, bounds.lightness.y, bounds.lightness.width, bounds.lightness.height);
  const radius = bounds.plane.width / 2,
    cmax = maxSrgbChroma(planeLightness, model.h),
    fraction = cmax ? Math.min(1, model.c / cmax) : 0,
    cx = bounds.plane.x + radius + radius * fraction * Math.cos(model.h),
    cy = bounds.plane.y + radius - radius * fraction * Math.sin(model.h);
  context.strokeStyle = "#fff";
  context.lineWidth = 2;
  context.beginPath();
  context.arc(cx, cy, 4, 0, Math.PI * 2);
  context.stroke();
  const y = bounds.lightness.y + bounds.lightness.height * (1 - model.l);
  context.strokeStyle = "#fff";
  context.strokeRect(bounds.lightness.x - 2, y - 2, bounds.lightness.width + 4, 4);
}

export function paintColorPicker(
  context: OffscreenCanvasRenderingContext2D,
  layout: ColorPickerLayout,
  model: Oklch,
  rgba: Rgba,
  hsv: readonly number[],
  edit?: { field: string; index: number; buffer: string; invalid: boolean },
  dpr = 1,
  target: DevicePaintTarget = { x: 0, y: 0, width: Number.MAX_SAFE_INTEGER, height: Number.MAX_SAFE_INTEGER },
): void {
  context.save();
  context.fillStyle = "#181a1f";
  context.strokeStyle = "#f5a623";
  context.lineWidth = 1;
  context.beginPath();
  context.roundRect(layout.bounds.x, layout.bounds.y, layout.bounds.width, layout.bounds.height, 7);
  context.fill();
  context.stroke();
  paintOklchWheel(context, layout, model, dpr, model.l, target);
  const alpha = context.createLinearGradient(0, layout.alpha.y, 0, layout.alpha.y + layout.alpha.height);
  alpha.addColorStop(0, `rgba(${rgba[0] * 255},${rgba[1] * 255},${rgba[2] * 255},1)`);
  alpha.addColorStop(1, `rgba(${rgba[0] * 255},${rgba[1] * 255},${rgba[2] * 255},0)`);
  context.fillStyle = alpha;
  context.fillRect(layout.alpha.x, layout.alpha.y, layout.alpha.width, layout.alpha.height);
  const y = layout.alpha.y + layout.alpha.height * (1 - rgba[3]);
  context.strokeStyle = "#fff";
  context.strokeRect(layout.alpha.x - 2, y - 2, layout.alpha.width + 4, 4);
  context.font = "12px sans-serif";
  context.textBaseline = "middle";
  context.textAlign = "center";
  context.fillStyle = "#eee";
  context.fillText("✓", layout.confirm.x + 12, layout.confirm.y + 12);
  const hex =
    "#" +
    rgba
      .map((v) =>
        Math.round(Math.max(0, Math.min(1, v)) * 255)
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
      .toUpperCase();
  for (const [rects, values, name, labels] of [
    [layout.rgba, rgba.map((v) => v.toFixed(3)), "rgba", "RGBA"],
    [layout.hsv, [hsv[0]!.toFixed(1), hsv[1]!.toFixed(3), hsv[2]!.toFixed(3)], "hsv", "HSV"],
  ] as const)
    rects.forEach((r, i) => {
      context.fillStyle = "#292c33";
      context.fillRect(r.x, r.y, r.width, r.height);
      context.strokeStyle = edit?.field === name && edit.index === i ? (edit.invalid ? "#e55" : "#f5a623") : "#555";
      context.strokeRect(r.x + 0.5, r.y + 0.5, r.width - 1, r.height - 1);
      context.fillStyle = "#eee";
      context.fillText(
        edit?.field === name && edit.index === i ? edit.buffer : `${labels[i]} ${values[i]}`,
        r.x + r.width / 2,
        r.y + r.height / 2,
      );
    });
  context.fillStyle = "#292c33";
  context.fillRect(layout.hex.x, layout.hex.y, layout.hex.width, layout.hex.height);
  context.strokeStyle = edit?.field === "hex" ? (edit.invalid ? "#e55" : "#f5a623") : "#555";
  context.strokeRect(layout.hex.x + 0.5, layout.hex.y + 0.5, layout.hex.width - 1, layout.hex.height - 1);
  context.fillStyle = "#eee";
  context.fillText(
    edit?.field === "hex" ? edit.buffer : `HEX ${hex}`,
    layout.hex.x + layout.hex.width / 2,
    layout.hex.y + layout.hex.height / 2,
  );
  context.restore();
}
