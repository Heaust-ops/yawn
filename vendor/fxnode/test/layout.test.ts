import test from "node:test";
import assert from "node:assert/strict";
import { APPLICATION_COMPILED, APPLICATION_HEADLESS } from "./application.js";
import { commandId, nodeId } from "@lib/core/types.js";
import { layoutGraph as genericLayoutGraph } from "@lib/layout/layout-graph.js";
import { viewToWorld, worldToView } from "@lib/layout/geometry.js";
const layoutGraph = (document: any, transform: Parameters<typeof genericLayoutGraph>[2]) =>
  genericLayoutGraph(APPLICATION_COMPILED, document as never, transform);
const { createEngine, emptyDocument, materializeNode, transition, socketsCompatible } = APPLICATION_HEADLESS;
const BUILTIN_DESCRIPTORS = [...APPLICATION_COMPILED.nodes.values()];
import type { GraphDocument } from "@lib/headless.js";
import { applyNodeOrder } from "@lib/layout/layout-graph.js";
import {
  boxNodes,
  clampResize,
  compatibleTargets,
  frameDropCandidate,
  groupRoots,
  hitRamp,
  hitTest,
  planLink,
  zoomAt,
} from "@lib/worker/interaction.js";

const transform = { center: { x: 0, y: 0 }, zoom: 2, viewport: { x: 800, y: 600 }, dpr: 1 };
test("world/view transforms round trip with +Y up", () => {
  const point = { x: 17, y: -23 };
  assert.deepEqual(viewToWorld(worldToView(point, transform), transform), point);
  assert.ok(worldToView({ x: 0, y: 1 }, transform).y < worldToView({ x: 0, y: 0 }, transform).y);
});
test("all catalog types lay out deterministically", () => {
  let state = createEngine(emptyDocument());
  for (const [index, descriptor] of BUILTIN_DESCRIPTORS.entries()) {
    const result = transition(state, {
      commandId: commandId(String(index)),
      expectedVersion: state.version,
      source: "api",
      command: {
        type: "node.add",
        nodeId: nodeId(`n${index}`),
        nodeType: descriptor.typeId,
        position: { x: index * 10, y: 0 },
      },
    });
    assert.notEqual(result.status, "rejected");
    if (result.status === "committed") state = result.state;
  }
  const a = layoutGraph(state.document, transform),
    b = layoutGraph(state.document, transform);
  assert.equal(a.nodes.size, BUILTIN_DESCRIPTORS.length);
  assert.deepEqual([...a.drawOrder], [...b.drawOrder]);
  const expected = [
    ["fxnode.common.frame", "frame", "common", [], [100, 100], [300, 100]],
    ["fxnode.common.reroute", "reroute", "common", ["socket:input:input:1"], [10, 10], [10, 10]],
    [
      "fxnode.shader.texture-coordinate",
      "node",
      "shader",
      ["socket:output:generated:1", "socket:output:normal:1", "socket:output:uv:1", "socket:output:object:1"],
      [192, 124],
      [192, 124],
    ],
    [
      "fxnode.shader.noise-texture",
      "node",
      "texture",
      [
        "control:enum:dimensions:1",
        "control:enum:noiseType:1",
        "control:boolean:normalize:1",
        "socket:input:vector:1",
        "socket:input:scale:1",
        "socket:input:detail:1",
        "socket:input:roughness:1",
        "socket:input:lacunarity:1",
        "socket:input:distortion:1",
        "socket:output:factor:1",
        "socket:output:color:1",
      ],
      [286, 292],
      [286, 292],
    ],
    [
      "fxnode.shader.image-texture",
      "node",
      "input",
      [
        "control:resource:image:4",
        "control:enum:interpolation:1",
        "control:enum:projection:1",
        "control:enum:extension:1",
        "control:enum:colorSpace:1",
        "control:enum:alphaMode:1",
        "socket:input:vector:1",
        "socket:output:color:1",
        "socket:output:alpha:1",
      ],
      [257, 316],
      [257, 316],
    ],
    [
      "fxnode.shader.principled-bsdf",
      "node",
      "shader",
      [
        "socket:input:base-color:1",
        "socket:input:metallic:1",
        "socket:input:roughness:1",
        "socket:input:ior:1",
        "socket:input:alpha:1",
        "socket:input:normal:1",
        "socket:output:bsdf:1",
      ],
      [208, 196],
      [208, 196],
    ],
    [
      "fxnode.shader.material-output",
      "node",
      "output",
      ["socket:input:surface:1", "socket:input:volume:1", "socket:input:displacement:1"],
      [340, 100],
      [340, 100],
    ],
    ["fxnode.geometry.position", "node", "geometry", ["socket:output:position:1"], [175, 52], [175, 100]],
    [
      "fxnode.geometry.mesh-cube",
      "node",
      "geometry",
      [
        "socket:input:size:1",
        "socket:input:vertices-x:1",
        "socket:input:vertices-y:1",
        "socket:input:vertices-z:1",
        "socket:output:mesh:1",
      ],
      [340, 148],
      [340, 148],
    ],
    [
      "fxnode.geometry.set-position",
      "node",
      "geometry",
      ["socket:input:geometry:1", "socket:input:position:1", "socket:input:offset:1", "socket:output:result:1"],
      [340, 124],
      [340, 124],
    ],
    [
      "fxnode.geometry.transform-geometry",
      "node",
      "geometry",
      [
        "socket:input:geometry:1",
        "socket:input:translation:1",
        "socket:input:rotation:1",
        "socket:input:scale:1",
        "socket:output:result:1",
      ],
      [340, 148],
      [340, 148],
    ],
    [
      "fxnode.geometry.join-geometry",
      "node",
      "geometry",
      ["socket:input:geometry:1", "socket:output:result:1"],
      [175, 76],
      [175, 100],
    ],
    [
      "fxnode.common.group-input",
      "node",
      "input",
      ["control:string:interfaceName:1", "socket:output:output:1"],
      [257, 76],
      [257, 100],
    ],
    [
      "fxnode.compositor.image",
      "node",
      "compositorInput",
      [
        "control:resource:image:4",
        "control:enum:source:1",
        "socket:output:image:1",
        "socket:output:alpha:1",
        "socket:output:z:1",
      ],
      [176, 220],
      [176, 220],
    ],
    [
      "fxnode.compositor.color-balance",
      "node",
      "compositorColor",
      [
        "control:enum:mode:1",
        "grading-wheels:Lift:number:lift:color:liftColor,Gamma:number:gamma:color:gammaColor,Gain:number:gain:color:gainColor:7",
        "socket:input:image:1",
        "socket:input:factor:1",
        "socket:output:result:1",
      ],
      [400, 292],
      [400, 292],
    ],
    [
      "fxnode.common.group-output",
      "node",
      "output",
      ["control:string:interfaceName:1", "socket:input:input:1"],
      [257, 76],
      [257, 100],
    ],
    [
      "fxnode.shader.value",
      "node",
      "shader",
      ["control:number:value:1", "socket:output:value:1"],
      [151, 76],
      [151, 100],
    ],
    [
      "fxnode.shader.color",
      "node",
      "shader",
      ["control:color:color:1", "socket:output:color:1"],
      [151, 76],
      [151, 100],
    ],
    [
      "fxnode.shader.math",
      "node",
      "converter",
      [
        "control:enum:operation:1",
        "control:boolean:clamp:1",
        "socket:input:a:1",
        "socket:input:b:1",
        "socket:output:value:1",
      ],
      [192, 148],
      [192, 148],
    ],
    [
      "fxnode.shader.vector-math",
      "node",
      "converter",
      [
        "control:enum:operation:1",
        "socket:input:a:1",
        "socket:input:b:1",
        "socket:output:vector:1",
        "socket:output:value:1",
      ],
      [340, 148],
      [340, 148],
    ],
    [
      "fxnode.shader.mix",
      "node",
      "shader",
      [
        "control:enum:blend:1",
        "control:boolean:clamp:1",
        "socket:input:factor:1",
        "socket:input:a:1",
        "socket:input:b:1",
        "socket:output:result:1",
      ],
      [151, 172],
      [151, 172],
    ],
    [
      "fxnode.shader.color-ramp",
      "node",
      "shader",
      ["socket:output:color:1", "socket:output:alpha:1", "control:color-ramp:ramp:8", "socket:input:factor:1"],
      [320, 292],
      [320, 292],
    ],
  ] as Array<[string, string, string, string[], [number, number], [number, number]]>;
  const layoutByType = new Map([...a.nodes.values()].map((node) => [node.typeId, node]));
  const aggregate = expected.map(([typeId]) => {
    const node = layoutByType.get(typeId)!;
    const sourceNode = state.document.nodes[node.id]!;
    return {
      typeId: node.typeId,
      kind: node.kind,
      category: node.styleId,
      rows: node.rows.map((row) => {
        if (row.kind === "control") {
          const control = a.controls.get(row.controlId)!;
          return `control:${control.kind}:${control.key}:${row.units}`;
        }
        if (row.kind === "socket") {
          const socket = sourceNode.sockets.find((item) => item.id === row.socketId)!;
          return `socket:${socket.direction}:${socket.key}:${row.units}`;
        }
        if (row.kind === "grading-wheels") {
          const wheels = row.wheels.map((wheel) => {
            const scalar = a.controls.get(wheel.scalarControlId)!;
            const color = a.controls.get(wheel.colorControlId)!;
            return `${wheel.label}:${scalar.kind}:${scalar.key}:${color.kind}:${color.key}`;
          });
          return `grading-wheels:${wheels.join(",")}:${row.units}`;
        }
        return `${row.kind}:${row.label}:${row.units}`;
      }),
      minimum: node.minimumSize,
      effective: { x: node.bounds.width, y: node.bounds.height },
    };
  });
  const expectedLayout = expected.map(
    ([typeId, kind, category, rows, [minimumX, minimumY], [effectiveX, effectiveY]]) => ({
      typeId,
      kind,
      category,
      rows,
      minimum: { x: minimumX, y: minimumY },
      effective: { x: effectiveX, y: effectiveY },
    }),
  );
  assert.deepEqual(aggregate, expectedLayout);
});

test("socket compatibility and exact theme palettes are stable", () => {
  const types = ["float", "vector", "color", "shader", "geometry", "any"] as const;
  const matrix = types.map((output) =>
    types.map((input) =>
      socketsCompatible(
        { direction: "output", dataType: output },
        { direction: "input", dataType: input, accepts: input === "any" ? ["any"] : [input, "any"] },
      ),
    ),
  );
  assert.deepEqual(matrix, [
    [true, false, false, false, false, true],
    [false, true, false, false, false, true],
    [false, false, true, false, false, true],
    [false, false, false, true, false, true],
    [false, false, false, false, true, true],
    [true, true, true, true, true, true],
  ]);
  assert.equal(
    socketsCompatible(
      { direction: "input", dataType: "float" },
      { direction: "input", dataType: "float", accepts: ["float"] },
    ),
    false,
  );
  assert.equal(
    socketsCompatible(
      { direction: "output", dataType: "float" },
      { direction: "output", dataType: "float", accepts: [] },
    ),
    false,
  );
  assert.deepEqual(Object.fromEntries([...APPLICATION_COMPILED.styles].map(([id, style]) => [id, style.header])), {
    input: "#8b3f72",
    converter: "#4f5964",
    texture: "#a36b34",
    shader: "#3b7551",
    output: "#963d3d",
    geometry: "#2c7a75",
    common: "#555b64",
    compositorInput: "#8b3f72",
    compositorColor: "#4f5964",
  });
  assert.deepEqual(Object.fromEntries([...APPLICATION_COMPILED.socketTypes].map(([id, type]) => [id, type.color])), {
    float: "#a8a8a8",
    vector: "#6476dc",
    color: "#d7ca63",
    shader: "#62b34f",
    geometry: "#00bfa5",
    any: "#999999",
  });
});

test("mute bypass map and endpoint anchors are exact", () => {
  const expected = [
    ["fxnode.common.reroute", [["input", "output"]]],
    ["fxnode.shader.math", [["a", "value"]]],
    ["fxnode.shader.vector-math", [["a", "vector"]]],
    ["fxnode.shader.mix", [["a", "result"]]],
    ["fxnode.geometry.set-position", [["geometry", "result"]]],
    ["fxnode.geometry.transform-geometry", [["geometry", "result"]]],
    ["fxnode.compositor.color-balance", [["image", "result"]]],
  ] as const;
  assert.deepEqual(
    BUILTIN_DESCRIPTORS.filter((item) => item.muteBypass.length).map((item) => [item.typeId, item.muteBypass]),
    expected,
  );
  for (const [typeId, pairs] of expected) {
    const node = { ...materializeNode("muted", typeId, { x: 0, y: 0 }), muted: true };
    const document: GraphDocument = { ...emptyDocument("mute"), nodes: { muted: node } };
    const layout = layoutGraph(document, transform),
      placed = layout.nodes.get(nodeId("muted"))!;
    assert.equal(placed.bypasses.length, pairs.length);
    for (const [index, [from, to]] of pairs.entries()) {
      const fromSocket = node.sockets.find((socket) => socket.key === from)!;
      const toSocket = node.sockets.find((socket) => socket.key === to)!;
      assert.deepEqual(placed.bypasses[index], {
        from: layout.sockets.get(fromSocket.id)!.anchor,
        to: layout.sockets.get(toSocket.id)!.anchor,
      });
    }
  }
});
test("expanded, collapsed, reroute and links have pinned geometry", () => {
  const value = materializeNode("value", "fxnode.shader.value", { x: -100, y: 20 });
  const math = materializeNode("math", "fxnode.shader.math", { x: 100, y: 20 });
  const link = {
    id: "link",
    fromNodeId: "value",
    fromSocketId: "value:value",
    toNodeId: "math",
    toSocketId: "math:a",
    extensions: {},
  };
  const raw = {
    schemaVersion: 1,
    graphId: "layout",
    catalogVersion: 1,
    nodes: { value, math },
    links: { link },
    metadata: {},
  } as unknown as Parameters<typeof layoutGraph>[0];
  const snapshot = layoutGraph(raw, transform);
  assert.equal(snapshot.links.values().next().value?.points.length, 13);
  const valueLayout = snapshot.nodes.get(nodeId("value"))!;
  assert.equal(valueLayout.bounds.width, valueLayout.minimumSize.x);
  assert.ok(valueLayout.bounds.width >= 140);
  assert.equal(snapshot.sockets.size, 4);
  assert.equal((snapshot.controls.get("value:parameter:value")?.value as { value: number }).value, 0);
  assert.equal((snapshot.controls.get("math:parameter:operation")?.value as { value: string }).value, "add");
  const mathLayout = snapshot.nodes.get(nodeId("math"))!;
  assert.ok(mathLayout.bounds.height >= 126);
  assert.equal(mathLayout.collapseHitRect.x, mathLayout.bounds.x);
  assert.equal(mathLayout.collapseHitRect.width, 14);
  assert.deepEqual(
    hitTest(snapshot, worldToView({ x: mathLayout.bounds.x + 10, y: mathLayout.bounds.y - 12 }, snapshot.transform)),
    { kind: "collapse", id: nodeId("math") },
  );
  assert.deepEqual(
    hitTest(snapshot, worldToView({ x: mathLayout.bounds.x + 22, y: mathLayout.bounds.y - 12 }, snapshot.transform)),
    { kind: "node", id: nodeId("math") },
  );
  assert.equal(snapshot.controls.get("math:socket:math:a")?.linked, true, "linked inputs hide controls");
});

test("frames have labelled fitted bounds around parent-local children", () => {
  const frame = {
    ...materializeNode("frame", "fxnode.common.frame", { x: -200, y: 200 }),
    label: "Surface Controls",
    size: { x: 100, y: 100 },
  };
  const child = { ...materializeNode("child", "fxnode.shader.value", { x: 30, y: -50 }), parentId: "frame" };
  const raw = {
    schemaVersion: 1,
    graphId: "frame-layout",
    catalogVersion: 1,
    nodes: { frame, child },
    links: {},
    metadata: {},
  } as unknown as Parameters<typeof layoutGraph>[0];
  const snapshot = layoutGraph(raw, transform),
    frameLayout = snapshot.nodes.get(nodeId("frame"))!,
    childLayout = snapshot.nodes.get(nodeId("child"))!;
  assert.equal(frameLayout.label, "Surface Controls");
  assert.ok(frameLayout.bounds.x <= childLayout.bounds.x - 30);
  assert.ok(frameLayout.bounds.x + frameLayout.bounds.width >= childLayout.bounds.x + childLayout.bounds.width + 30);
  assert.ok(frameLayout.bounds.y >= childLayout.bounds.y + 30);
  assert.ok(frameLayout.bounds.y - frameLayout.bounds.height <= childLayout.bounds.y - childLayout.bounds.height - 30);
});
test("socket compatibility does not treat accepts any as wildcard", () => {
  const output = { direction: "output" as const, dataType: "float" as const };
  assert.equal(socketsCompatible(output, { direction: "input", dataType: "vector", accepts: ["any"] }), false);
  assert.equal(socketsCompatible(output, { direction: "input", dataType: "any", accepts: [] }), true);
});
test("wheel zoom preserves world point and group roots omit selected descendants", () => {
  const cursor = { x: 123, y: 234 },
    before = viewToWorld(cursor, transform),
    next = zoomAt(transform, cursor, 120),
    after = viewToWorld(cursor, { ...transform, ...next });
  assert.ok(Math.abs(before.x - after.x) < 1e-9);
  assert.ok(Math.abs(before.y - after.y) < 1e-9);
  const frame = { ...materializeNode("f", "fxnode.common.frame", { x: 0, y: 0 }) },
    child = { ...materializeNode("c", "fxnode.shader.value", { x: 10, y: -10 }), parentId: nodeId("f") };
  const doc = {
    schemaVersion: 1,
    graphId: "g",
    catalogVersion: 1,
    nodes: { f: frame, c: child },
    links: {},
    metadata: {},
  } as unknown as Parameters<typeof layoutGraph>[0];
  assert.deepEqual(groupRoots(new Set([nodeId("f"), nodeId("c")]), layoutGraph(doc, transform)), [nodeId("f")]);
});
test("transient node order controls overlapping paint and hit order", () => {
  const a = materializeNode("a", "fxnode.shader.value", { x: 0, y: 0 }),
    b = materializeNode("b", "fxnode.shader.value", { x: 0, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "z",
      catalogVersion: 1,
      nodes: { a, b },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0];
  const original = layoutGraph(doc, { ...transform, zoom: 1 }),
    raised = applyNodeOrder(original, [nodeId("b"), nodeId("a")]),
    point = worldToView({ x: 40, y: -20 }, raised.transform);
  assert.deepEqual(raised.drawOrder.slice(-2), [nodeId("b"), nodeId("a")]);
  assert.deepEqual(hitTest(raised, point), { kind: "node", id: nodeId("a") });
  const shifted = { ...b, position: { x: 30, y: 0 } },
    overlap = applyNodeOrder(layoutGraph({ ...doc, nodes: { a, b: shifted } }, { ...transform, zoom: 1 }), [
      nodeId("a"),
      nodeId("b"),
    ]);
  assert.deepEqual(hitTest(overlap, worldToView({ x: 35, y: -36 }, overlap.transform)), {
    kind: "node",
    id: nodeId("b"),
  });
});
test("reroute core starts links while its outer halo selects the node", () => {
  const reroute = materializeNode("r", "fxnode.common.reroute", { x: 0, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "reroute",
      catalogVersion: 1,
      nodes: { r: reroute },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, { ...transform, zoom: 1 }),
    node = layout.nodes.get(nodeId("r"))!,
    center = worldToView({ x: node.bounds.x + 5, y: node.bounds.y - 5 }, layout.transform);
  assert.equal(hitTest(layout, center, "output").kind, "socket");
  assert.deepEqual(hitTest(layout, { x: center.x + 11, y: center.y }), { kind: "node", id: nodeId("r") });
});
test("interaction helpers hit sampled links, box nodes, plan replacement and clamp resize", () => {
  const value = materializeNode("v", "fxnode.shader.value", { x: -100, y: 50 }),
    math = materializeNode("m", "fxnode.shader.math", { x: 100, y: 50 });
  const old = { id: "old", fromNodeId: "v", fromSocketId: "v:value", toNodeId: "m", toSocketId: "m:a", extensions: {} };
  const doc = {
      schemaVersion: 1,
      graphId: "i",
      catalogVersion: 1,
      nodes: { v: value, m: math },
      links: { old },
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, { ...transform, zoom: 1 });
  const link = layout.links.values().next().value!,
    mid = worldToView(link.points[6]!, layout.transform);
  assert.deepEqual(hitTest(layout, mid), { kind: "link", id: "old" });
  const v = layout.nodes.get(nodeId("v"))!;
  assert.deepEqual(
    boxNodes(
      layout,
      worldToView({ x: v.bounds.x - 1, y: v.bounds.y + 1 }, layout.transform),
      worldToView({ x: v.bounds.x + v.bounds.width + 1, y: v.bounds.y - v.bounds.height - 1 }, layout.transform),
    ),
    [nodeId("v")],
  );
  assert.equal(
    compatibleTargets(layout, "v:value" as never).some((s) => s.id === "m:a"),
    true,
  );
  assert.equal(planLink(layout, "v:value" as never, "m:a" as never)?.type, "link.replace");
  assert.deepEqual(clampResize(layout, nodeId("m"), { x: 10000, y: 10000 }), {
    x: 700,
    y: layout.nodes.get(nodeId("m"))!.minimumSize.y,
  });
});
test("frame drop picks containing frame and rejects self", () => {
  const frame = materializeNode("f", "fxnode.common.frame", { x: -200, y: 200 }),
    node = materializeNode("n", "fxnode.shader.value", { x: 0, y: 0 });
  const doc = {
      schemaVersion: 1,
      graphId: "d",
      catalogVersion: 1,
      nodes: { f: frame, n: node },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, transform);
  assert.equal(frameDropCandidate(layout, nodeId("n"), { x: -100, y: 100 }), nodeId("f"));
  assert.equal(frameDropCandidate(layout, nodeId("f"), { x: -100, y: 100 }), undefined);
});
test("Color Ramp authoritative bounds resolve every interaction region", () => {
  const node = materializeNode("r", "fxnode.shader.color-ramp", { x: 0, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "r",
      catalogVersion: 1,
      nodes: { r: node },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, { ...transform, zoom: 1 }),
    control = layout.controls.get("r:parameter:ramp")!,
    b = control.rampBounds!,
    center = (r: typeof b.toolbar) => ({ x: r.x + r.width / 2, y: r.y - r.height / 2 });
  assert.equal(hitRamp(control, { x: b.toolbar.x + 1, y: b.toolbar.y - 10 })?.target, "add");
  assert.equal(hitRamp(control, { x: b.toolbar.x + b.toolbar.width * 0.18, y: b.toolbar.y - 10 })?.target, "remove");
  assert.equal(hitRamp(control, { x: b.toolbar.x + b.toolbar.width * 0.4, y: b.toolbar.y - 10 })?.target, "flip");
  assert.equal(hitRamp(control, { x: b.toolbar.x + b.toolbar.width * 0.8, y: b.toolbar.y - 10 })?.target, "distribute");
  assert.equal(hitRamp(control, center(b.mode))?.target, "mode");
  assert.equal(hitRamp(control, center(b.interpolation))?.target, "interpolation");
  assert.equal(hitRamp(control, center(b.hue))?.target, "hue");
  assert.equal(hitRamp(control, center(b.gradient))?.target, "gradient");
  assert.equal(hitRamp(control, center(b.selector))?.target, "selector");
  assert.equal(hitRamp(control, center(b.position))?.target, "position");
  for (const x of [0.05, 0.5, 0.95])
    assert.equal(hitRamp(control, { x: b.color.x + b.color.width * x, y: b.color.y - 10 })?.target, "swatch");
});

test("Color Balance owns three disjoint Blender-style grading wheels", () => {
  const node = materializeNode("balance", "fxnode.compositor.color-balance", { x: 0, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "balance",
      catalogVersion: 1,
      nodes: { balance: node },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, { ...transform, zoom: 1 }),
    placed = layout.nodes.get(nodeId("balance"))!,
    row = placed.rows.find((item) => item.kind === "grading-wheels");
  assert.ok(row && row.kind === "grading-wheels");
  assert.deepEqual(
    row.wheels.map((wheel) => wheel.label),
    ["Lift", "Gamma", "Gain"],
  );
  assert.ok(placed.minimumSize.x >= 400);
  assert.equal(row.units, 7);
  for (const wheel of row.wheels) {
    const color = layout.controls.get(wheel.colorControlId)!,
      scalar = layout.controls.get(wheel.scalarControlId)!,
      bounds = color.colorWheelBounds!;
    assert.equal(bounds.plane.width, bounds.plane.height);
    assert.ok(bounds.lightness.x >= bounds.plane.x + bounds.plane.width);
    for (const [region, rect] of Object.entries(bounds) as ["plane" | "lightness", typeof bounds.plane][])
      assert.deepEqual(
        hitTest(layout, worldToView({ x: rect.x + rect.width / 2, y: rect.y - rect.height / 2 }, layout.transform)),
        { kind: "color-wheel", id: color.id, region },
      );
    assert.equal(
      hitTest(
        layout,
        worldToView(
          { x: scalar.bounds.x + scalar.bounds.width / 2, y: scalar.bounds.y - scalar.bounds.height / 2 },
          layout.transform,
        ),
      ).kind,
      "control",
    );
  }
  for (let i = 1; i < row.wheels.length; i++) {
    const prior = layout.controls.get(row.wheels[i - 1]!.colorControlId)!.bounds,
      next = layout.controls.get(row.wheels[i]!.colorControlId)!.bounds;
    assert.ok(next.x >= prior.x + prior.width);
  }
  const collapsed = layoutGraph(
    { ...doc, nodes: { balance: { ...node, collapsed: true } } },
    { ...transform, zoom: 1 },
  ).nodes.get(nodeId("balance"))!;
  assert.equal(collapsed.bounds.width, placed.bounds.width);
  assert.equal(collapsed.bounds.height, 24);
});

test("compound and component controls have bounded, disjoint layout cells", () => {
  const types = [
    ["r", "fxnode.shader.color-ramp"],
    ["i", "fxnode.shader.image-texture"],
    ["g", "fxnode.compositor.color-balance"],
  ] as const;
  const nodes = Object.fromEntries(
    types.map(([id, type], index) => [id, materializeNode(id, type, { x: index * 500, y: 0 })]),
  );
  const doc = {
      schemaVersion: 1,
      graphId: "cells",
      catalogVersion: 1,
      nodes,
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, transform);
  const inside = (
    outer: { x: number; y: number; width: number; height: number },
    inner: { x: number; y: number; width: number; height: number },
  ) =>
    inner.x >= outer.x &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y <= outer.y &&
    inner.y - inner.height >= outer.y - outer.height;
  for (const control of layout.controls.values()) {
    const node = layout.nodes.get(control.nodeId)!;
    assert.ok(inside(node.bounds, control.bounds), `${control.id} is inside its node`);
    for (let index = 1; index < control.subfields.length; index++)
      assert.ok(
        control.subfields[index]!.bounds.x -
          (control.subfields[index - 1]!.bounds.x + control.subfields[index - 1]!.bounds.width) >=
          2,
        "component gutter",
      );
  }
  const ramp = layout.controls.get("r:parameter:ramp")!,
    rampRow = layout.nodes.get(nodeId("r"))!.rows.find((row) => row.kind === "control")!;
  for (const row of layout.nodes.get(nodeId("r"))!.rows.filter((row) => row.kind === "socket"))
    assert.ok(
      row.bounds.y - row.bounds.height >= rampRow.bounds.y || rampRow.bounds.y - rampRow.bounds.height >= row.bounds.y,
      "ramp and sockets disjoint",
    );
  const ordinary = [layout.controls.get("i:parameter:interpolation")!, layout.controls.get("i:parameter:projection")!],
    factor = layout.controls.get("g:socket:g:factor")!;
  assert.ok([...ordinary, factor].every((control) => control.bounds.height === 18));
  assert.ok(
    ordinary.every(
      (control) =>
        Math.abs(
          (control.bounds.x - layout.nodes.get(control.nodeId)!.bounds.x) /
            layout.nodes.get(control.nodeId)!.bounds.width -
            0.42,
        ) < 1e-9,
    ),
  );
  assert.equal(factor.bounds.x - layout.nodes.get(factor.nodeId)!.bounds.x, 12);
});

test("numeric fields expose Blender-style range fill and step geometry", () => {
  const principled = materializeNode("p", "fxnode.shader.principled-bsdf", { x: 0, y: 0 }),
    valueNode = materializeNode("v", "fxnode.shader.value", { x: 500, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "numeric-fields",
      catalogVersion: 1,
      nodes: { p: principled, v: valueNode },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, transform),
    roughness = layout.controls.get("p:socket:p:roughness")!.numericFields[0]!,
    unbounded = layout.controls.get("v:parameter:value")!.numericFields[0]!;
  assert.deepEqual(roughness.range, { minimum: 0, maximum: 1 });
  assert.equal(unbounded.range, undefined);
  assert.equal(roughness.decrement.width, 7);
  assert.equal(roughness.increment.width, 7);
  assert.ok(roughness.value.x >= roughness.decrement.x + roughness.decrement.width);
  assert.ok(roughness.increment.x >= roughness.value.x + roughness.value.width);
});

test("color controls are compact swatches rather than RGBA fields", () => {
  const colorNode = materializeNode("c", "fxnode.shader.color", { x: 0, y: 0 }),
    principled = materializeNode("p", "fxnode.shader.principled-bsdf", { x: 300, y: 0 }),
    doc = {
      schemaVersion: 1,
      graphId: "swatches",
      catalogVersion: 1,
      nodes: { c: colorNode, p: principled },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(doc, transform),
    parameter = layout.controls.get("c:parameter:color")!,
    socket = layout.controls.get("p:socket:p:base-color")!;
  for (const control of [parameter, socket]) {
    assert.equal(control.kind, "color");
    assert.equal(control.subfields.length, 0);
    assert.equal(control.numericFields.length, 0);
    assert.equal(
      hitTest(
        layout,
        worldToView(
          { x: control.bounds.x + control.bounds.width / 2, y: control.bounds.y - control.bounds.height / 2 },
          layout.transform,
        ),
      ).kind,
      "control",
    );
  }
  assert.equal(layout.nodes.get(nodeId("c"))!.minimumSize.x, 151);
  assert.equal(layout.nodes.get(nodeId("p"))!.bounds.width, 208);
});

test("resource previews and open buttons are authoritative worker hit targets", () => {
  const image = materializeNode("i", "fxnode.shader.image-texture", { x: 0, y: 0 }),
    document = {
      schemaVersion: 1,
      graphId: "resource-hits",
      catalogVersion: 1,
      nodes: { i: image },
      links: {},
      metadata: {},
    } as unknown as Parameters<typeof layoutGraph>[0],
    layout = layoutGraph(document, transform),
    control = layout.controls.get("i:parameter:image")!,
    bounds = control.resourceBounds!,
    center = (rect: typeof bounds.preview) =>
      worldToView({ x: rect.x + rect.width / 2, y: rect.y - rect.height / 2 }, layout.transform);
  assert.equal(control.kind, "resource");
  assert.deepEqual(hitTest(layout, center(bounds.preview)), { kind: "resource", id: control.id });
  assert.deepEqual(hitTest(layout, center(bounds.open)), { kind: "resource", id: control.id });
});
