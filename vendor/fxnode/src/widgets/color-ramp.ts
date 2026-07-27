/**
 * Immutable color-ramp model, mutations, migration, and sampling.
 *
 * Valid ramps contain 2–32 uniquely identified, position-sorted stops. Mutations
 * preserve those invariants and clamp finite positions/channels to [0, 1]. `hsl`
 * is currently identical to `hsv` (it is not HSL). `b-spline` uses the same easing
 * as `ease`; `cardinal` is a two-stop approximation without neighbourhood spline
 * evaluation.
 * @module fxnode/widgets/color-ramp
 */
export type ColorRampMode = "rgb" | "hsv" | "hsl";
export type ColorRampInterpolation = "linear" | "ease" | "constant" | "cardinal" | "b-spline";
export type HueInterpolation = "near" | "far" | "clockwise" | "counter-clockwise";
export interface ColorRampStop {
  readonly id: string;
  readonly position: number;
  readonly color: readonly [number, number, number, number];
}
export interface ColorRamp {
  readonly colorMode: ColorRampMode;
  readonly interpolation: ColorRampInterpolation;
  readonly hueInterpolation: HueInterpolation;
  readonly stops: readonly ColorRampStop[];
}
const clamp = (n: number) => Math.max(0, Math.min(1, n));
export function isColorRamp(value: unknown): value is ColorRamp {
  if (!value || typeof value !== "object") return false;
  const r = value as ColorRamp;
  if (
    !(["rgb", "hsv", "hsl"] as unknown[]).includes(r.colorMode) ||
    !(["linear", "ease", "constant", "cardinal", "b-spline"] as unknown[]).includes(r.interpolation) ||
    !(["near", "far", "clockwise", "counter-clockwise"] as unknown[]).includes(r.hueInterpolation) ||
    !Array.isArray(r.stops) ||
    r.stops.length < 2 ||
    r.stops.length > 32
  )
    return false;
  let previous = -Infinity;
  const ids = new Set<string>();
  return r.stops.every(
    (s) =>
      typeof s?.id === "string" &&
      s.id.length > 0 &&
      !ids.has(s.id) &&
      !!ids.add(s.id) &&
      Number.isFinite(s.position) &&
      s.position >= 0 &&
      s.position <= 1 &&
      s.position >= previous &&
      (previous = s.position) >= 0 &&
      Array.isArray(s.color) &&
      s.color.length === 4 &&
      s.color.every((c: number) => Number.isFinite(c) && c >= 0 && c <= 1),
  );
}
export function migrateColorRamp(value: unknown): ColorRamp | undefined {
  const raw =
    value &&
    typeof value === "object" &&
    "kind" in value &&
    (value as { kind?: unknown }).kind === "json" &&
    "value" in value
      ? (value as unknown as { value: unknown }).value
      : value;
  if (isColorRamp(raw)) return raw;
  if (!Array.isArray(raw) || raw.length < 2 || raw.length > 32) return;
  const stops = raw.map((s, i) => {
    const x = s as { id?: unknown; position?: unknown; color?: unknown };
    return { id: typeof x?.id === "string" && x.id ? x.id : `stop-${i}`, position: x?.position, color: x?.color };
  });
  const candidate = { colorMode: "rgb", interpolation: "linear", hueInterpolation: "near", stops };
  return isColorRamp(candidate) ? candidate : undefined;
}
const sorted = (r: ColorRamp, stops: readonly ColorRampStop[]): ColorRamp => ({
  ...r,
  stops: [...stops].sort((a, b) => a.position - b.position),
});
/** Selects the indexed stop at an exact position, or returns `undefined`. */
export const selectRampStop = (r: ColorRamp, position: number, index = 0): string | undefined =>
  r.stops.filter((s) => s.position === position)[index]?.id;
/** Adds a sampled stop, preserving ramp validity; invalid IDs/positions are no-ops. */
export function addRampStop(r: ColorRamp, position: number, id: string): ColorRamp {
  if (!id || !Number.isFinite(position) || r.stops.length >= 32 || r.stops.some((s) => s.id === id)) return r;
  return sorted(r, [...r.stops, { id, position: clamp(position), color: sampleColorRamp(r, position) }]);
}
/** Blender's plus inserts halfway between active and its left neighbour (or right neighbour for the first stop). */
export function addRampMidpoint(r: ColorRamp, activeId: string, id: string): ColorRamp {
  const i = r.stops.findIndex((s) => s.id === activeId),
    a = r.stops[i];
  if (!a) return r;
  const other = r.stops[i > 0 ? i - 1 : i + 1];
  return other ? addRampStop(r, (a.position + other.position) / 2, id) : r;
}
export function removeRampStop(r: ColorRamp, id: string): ColorRamp {
  return r.stops.length <= 2 ? r : { ...r, stops: r.stops.filter((s) => s.id !== id) };
}
export function moveRampStop(r: ColorRamp, id: string, position: number): ColorRamp {
  if (!Number.isFinite(position)) return r;
  return sorted(
    r,
    r.stops.map((s) => (s.id === id ? { ...s, position: clamp(position) } : s)),
  );
}
export function setRampColor(r: ColorRamp, id: string, color: readonly [number, number, number, number]): ColorRamp {
  if (!color.every(Number.isFinite)) return r;
  return {
    ...r,
    stops: r.stops.map((s) =>
      s.id === id ? { ...s, color: color.map(clamp) as unknown as readonly [number, number, number, number] } : s,
    ),
  };
}
export function flipColorRamp(r: ColorRamp): ColorRamp {
  return sorted(
    r,
    r.stops.map((s) => ({ ...s, position: 1 - s.position })),
  );
}
export function distributeColorRamp(r: ColorRamp): ColorRamp {
  const first = r.stops[0]!.position,
    last = r.stops.at(-1)!.position,
    n = r.stops.length - 1;
  return { ...r, stops: r.stops.map((s, i) => ({ ...s, position: first + ((last - first) * i) / n })) };
}
const rgbToHsv = (c: readonly number[]) => {
  const [r, g, b] = c,
    m = Math.max(r!, g!, b!),
    n = Math.min(r!, g!, b!),
    d = m - n;
  let h = 0;
  if (d) h = m === r ? ((g! - b!) / d + 6) % 6 : m === g ? (b! - r!) / d + 2 : (r! - g!) / d + 4;
  return [h / 6, m ? d / m : 0, m];
};
const hsvToRgb = ([h, s, v]: readonly number[]) => {
  const wrapped = ((h! % 1) + 1) % 1,
    i = Math.floor(wrapped * 6),
    f = wrapped * 6 - i,
    p = v! * (1 - s!),
    q = v! * (1 - f * s!),
    t = v! * (1 - (1 - f) * s!);
  return (
    [
      [v, t, p],
      [q, v, p],
      [p, v, t],
      [p, q, v],
      [t, p, v],
      [v, p, q],
    ] as number[][]
  )[i % 6]!;
};
function hueDelta(a: number, b: number, mode: HueInterpolation) {
  let d = (((b - a) % 1) + 1) % 1;
  if (mode === "near" && d > 0.5) d -= 1;
  if (mode === "far" && d < 0.5) d -= 1;
  if (mode === "counter-clockwise" && d > 0) d -= 1;
  return d;
}
/** Samples a valid ramp; non-finite positions safely sample its first stop. */
export function sampleColorRamp(r: ColorRamp, position: number): readonly [number, number, number, number] {
  if (!Number.isFinite(position)) return r.stops[0]!.color;
  const p = clamp(position),
    right = r.stops.findIndex((s) => s.position >= p);
  if (right <= 0) return r.stops[Math.max(0, right)]!.color;
  const b = r.stops[right] ?? r.stops.at(-1)!,
    a = r.stops[right - 1]!,
    span = b.position - a.position;
  let t = span ? (p - a.position) / span : 1;
  if (r.interpolation === "constant") t = 0;
  else if (r.interpolation === "ease") t = t * t * (3 - 2 * t);
  else if (r.interpolation === "cardinal") t = t * t * (2 - t);
  else if (r.interpolation === "b-spline") t = t * t * (3 - 2 * t);
  let av = [...a.color.slice(0, 3)],
    bv = [...b.color.slice(0, 3)];
  if (r.colorMode !== "rgb") {
    av = rgbToHsv(av);
    bv = rgbToHsv(bv);
    av[0] = av[0]! + hueDelta(av[0]!, bv[0]!, r.hueInterpolation) * t;
    const rgb = hsvToRgb([av[0], av[1]! + (bv[1]! - av[1]!) * t, av[2]! + (bv[2]! - av[2]!) * t]);
    return [clamp(rgb[0]!), clamp(rgb[1]!), clamp(rgb[2]!), a.color[3] + (b.color[3] - a.color[3]) * t];
  }
  return [
    clamp(av[0]! + (bv[0]! - av[0]!) * t),
    clamp(av[1]! + (bv[1]! - av[1]!) * t),
    clamp(av[2]! + (bv[2]! - av[2]!) * t),
    clamp(a.color[3] + (b.color[3] - a.color[3]) * t),
  ];
}
