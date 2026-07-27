export type Rgb = readonly [number, number, number];
export type Rgba = readonly [number, number, number, number];
export interface Oklab {
  readonly l: number;
  readonly a: number;
  readonly b: number;
}
export interface Oklch {
  readonly l: number;
  readonly c: number;
  readonly h: number;
}
const tau = Math.PI * 2;
const decode = (value: number) =>
  Math.abs(value) <= 0.04045 ? value / 12.92 : Math.sign(value) * ((Math.abs(value) + 0.055) / 1.055) ** 2.4;
const encode = (value: number) =>
  Math.abs(value) <= 0.0031308 ? 12.92 * value : Math.sign(value) * (1.055 * Math.abs(value) ** (1 / 2.4) - 0.055);
export const normalizeHue = (value: number) => ((value % tau) + tau) % tau;
export function srgbToOklab(rgb: Rgb): Oklab {
  const r = decode(rgb[0]),
    g = decode(rgb[1]),
    b = decode(rgb[2]),
    l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b),
    m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b),
    s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return {
    l: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    a: 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    b: 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  };
}
export function oklabToLinearSrgb({ l, a, b }: Oklab): Rgb {
  const x = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3,
    y = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3,
    z = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * x - 3.3077115913 * y + 0.2309699292 * z,
    -1.2684380046 * x + 2.6097574011 * y - 0.3413193965 * z,
    -0.0041960863 * x - 0.7034186147 * y + 1.707614701 * z,
  ];
}
export function oklabToOklch(value: Oklab, fallbackHue = 0): Oklch {
  const c = Math.hypot(value.a, value.b);
  return { l: value.l, c, h: c <= 4e-6 ? normalizeHue(fallbackHue) : normalizeHue(Math.atan2(value.b, value.a)) };
}
export const oklchToOklab = ({ l, c, h }: Oklch): Oklab => ({ l, a: c * Math.cos(h), b: c * Math.sin(h) });
const inLinearGamut = (rgb: Rgb) => rgb.every((value) => Number.isFinite(value) && value >= -1e-9 && value <= 1 + 1e-9);
export const isInSrgbGamut = (color: Oklch) => inLinearGamut(oklabToLinearSrgb(oklchToOklab(color)));
export function maxSrgbChroma(l: number, h: number): number {
  if (l <= 0 || l >= 1) return 0;
  let low = 0,
    high = 0.4;
  while (high < 2 && isInSrgbGamut({ l, c: high, h })) high *= 2;
  for (let i = 0; i < 20; i++) {
    const middle = (low + high) / 2;
    if (isInSrgbGamut({ l, c: middle, h })) low = middle;
    else high = middle;
  }
  return low;
}
export function mapOklchToSrgb(color: Oklch): Rgb {
  if (color.l <= 0) return [0, 0, 0];
  if (color.l >= 1) return [1, 1, 1];
  const mapped = { ...color, c: Math.min(Math.max(0, color.c), maxSrgbChroma(color.l, color.h)) },
    linear = oklabToLinearSrgb(oklchToOklab(mapped));
  return linear.map((value) => Math.min(1, Math.max(0, encode(value)))) as unknown as Rgb;
}
