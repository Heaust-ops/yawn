import test from "node:test";
import assert from "node:assert/strict";
import { isInSrgbGamut, mapOklchToSrgb, maxSrgbChroma, oklabToOklch, srgbToOklab } from "@lib/color/oklab.js";
test("Oklab conversion matches reference primaries", () => {
  const red = srgbToOklab([1, 0, 0]);
  assert.ok(Math.abs(red.l - 0.627955) < 1e-5);
  assert.ok(Math.abs(red.a - 0.224863) < 1e-5);
  assert.ok(Math.abs(red.b - 0.125846) < 1e-5);
  const white = srgbToOklab([1, 1, 1]);
  assert.ok(Math.abs(white.l - 1) < 1e-6);
});
test("Oklch gamut mapping preserves lightness and hue by reducing chroma", () => {
  const requested = { l: 0.65, c: 0.6, h: 1.2 },
    limit = maxSrgbChroma(requested.l, requested.h);
  assert.ok(limit < requested.c);
  assert.equal(isInSrgbGamut({ ...requested, c: limit }), true);
  const rgb = mapOklchToSrgb(requested);
  assert.ok(rgb.every((value) => value >= 0 && value <= 1));
  const mapped = oklabToOklch(srgbToOklab(rgb));
  assert.ok(Math.abs(mapped.l - requested.l) < 2e-5);
  assert.ok(Math.abs(mapped.h - requested.h) < 2e-4);
});
