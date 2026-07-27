import test from "node:test";
import assert from "node:assert/strict";
import {
  addRampMidpoint,
  addRampStop,
  distributeColorRamp,
  flipColorRamp,
  isColorRamp,
  migrateColorRamp,
  moveRampStop,
  removeRampStop,
  sampleColorRamp,
  setRampColor,
  type ColorRamp,
} from "@lib/widgets/color-ramp.js";
const DEFAULT_COLOR_RAMP: ColorRamp = {
  colorMode: "rgb",
  interpolation: "linear",
  hueInterpolation: "near",
  stops: [
    { id: "stop-0", position: 0, color: [0, 0, 0, 1] },
    { id: "stop-1", position: 1, color: [1, 1, 1, 1] },
  ],
};
import { layoutGraph as genericLayoutGraph } from "@lib/layout/layout-graph.js";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";
const layoutGraph = (
  document: Parameters<typeof genericLayoutGraph>[1],
  transform: Parameters<typeof genericLayoutGraph>[2],
) => genericLayoutGraph(APPLICATION_COMPILED, document, transform);
const { materializeNode } = APPLICATION_HEADLESS;

test("Color Ramp migration, validation and stable pure operations", () => {
  const migrated = migrateColorRamp([
    { position: 0, color: [0, 0, 0, 1] },
    { position: 0.5, color: [1, 0, 0, 1] },
    { position: 1, color: [1, 1, 1, 1] },
  ])!;
  assert.deepEqual(
    migrated.stops.map((s) => s.id),
    ["stop-0", "stop-1", "stop-2"],
  );
  assert.equal(isColorRamp(migrated), true);
  const added = addRampStop(migrated, 0.25, "worker-1");
  assert.equal(added.stops.find((s) => s.id === "worker-1")?.color[0], 0.5);
  assert.equal(addRampMidpoint(migrated, "stop-1", "worker-2").stops.find((s) => s.id === "worker-2")?.position, 0.25);
  assert.deepEqual(
    moveRampStop(migrated, "stop-0", 0.75).stops.map((s) => s.id),
    ["stop-1", "stop-0", "stop-2"],
  );
  assert.equal(removeRampStop(DEFAULT_COLOR_RAMP, "stop-0"), DEFAULT_COLOR_RAMP);
  assert.deepEqual(
    distributeColorRamp(migrated).stops.map((s) => s.position),
    [0, 0.5, 1],
  );
  assert.deepEqual(
    flipColorRamp(migrated).stops.map((s) => s.position),
    [0, 0.5, 1],
  );
  assert.deepEqual(sampleColorRamp({ ...DEFAULT_COLOR_RAMP, interpolation: "constant" }, 0.5), [0, 0, 0, 1]);
});

test("Color Ramp public operations preserve validity for empty IDs and non-finite numbers", () => {
  for (const position of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
    assert.equal(addRampStop(DEFAULT_COLOR_RAMP, position, "new"), DEFAULT_COLOR_RAMP);
    assert.equal(moveRampStop(DEFAULT_COLOR_RAMP, "stop-0", position), DEFAULT_COLOR_RAMP);
    assert.deepEqual(sampleColorRamp(DEFAULT_COLOR_RAMP, position), DEFAULT_COLOR_RAMP.stops[0]!.color);
  }
  assert.equal(addRampStop(DEFAULT_COLOR_RAMP, 0.5, ""), DEFAULT_COLOR_RAMP);
  for (let channel = 0; channel < 4; channel++) {
    const color = [0, 0, 0, 1] as number[];
    color[channel] = Number.NaN;
    assert.equal(
      setRampColor(DEFAULT_COLOR_RAMP, "stop-0", color as [number, number, number, number]),
      DEFAULT_COLOR_RAMP,
    );
  }
  assert.equal(isColorRamp(DEFAULT_COLOR_RAMP), true);
});

test("Noise Texture exhaustive Blender 4.5 visibility matrix and immediate height", () => {
  const dimensions = ["1d", "2d", "3d", "4d"],
    types = ["fbm", "multifractal", "hybrid-multifractal", "ridged-multifractal", "hetero-terrain"];
  for (const dimension of dimensions)
    for (const noiseType of types) {
      const base = materializeNode("noise", "fxnode.shader.noise-texture");
      const node = {
        ...base,
        parameters: {
          ...base.parameters,
          dimensions: { kind: "string" as const, value: dimension },
          noiseType: { kind: "string" as const, value: noiseType },
        },
      };
      const document = {
        schemaVersion: 2 as const,
        graphId: "g" as never,
        catalogVersion: 2,
        nodes: { noise: node },
        links: {},
        metadata: {},
      };
      const layout = layoutGraph(document, { center: { x: 0, y: 0 }, zoom: 1, viewport: { x: 800, y: 600 }, dpr: 1 });
      const keys = new Set([...layout.sockets.values()].map((s) => s.id.split(":").at(-1)));
      assert.equal(keys.has("vector"), dimension !== "1d");
      assert.equal(keys.has("w"), dimension === "1d" || dimension === "4d");
      assert.equal(
        keys.has("offset"),
        ["hybrid-multifractal", "ridged-multifractal", "hetero-terrain"].includes(noiseType),
      );
      assert.equal(keys.has("gain"), ["hybrid-multifractal", "ridged-multifractal"].includes(noiseType));
      const hasNormalize = layout.controls.has("noise:parameter:normalize");
      assert.equal(hasNormalize, noiseType === "fbm");
      assert.ok(layout.nodes.get("noise" as never)!.bounds.height > 0);
    }
});
