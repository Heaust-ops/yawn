import assert from "node:assert/strict";
import test from "node:test";
import { compileFxNodeComposition } from "@lib/composition/index.js";
import { bindFxNodeHeadless } from "@lib/headless-runtime.js";
import { layoutGraph } from "@lib/layout/layout-graph.js";
import { effectivelyMutedLinks } from "@lib/layout/link-mute.js";
import { IndexedLayoutStore } from "@lib/layout/indexed-layout-store.js";
import { layoutSocketsCompatible } from "@lib/layout/types.js";
import { compatibleTargets } from "@lib/worker/interaction.js";

const theme = {
  background: "#010203",
  grid: "#040506",
  frame: "#070809",
  frameHeader: "#0a0b0c",
  body: "#0d0e0f",
  control: "#101112",
  controlFill: "#131415",
  controlEditing: "#161718",
  textSelection: "#191a1b",
  outline: "#1c1d1e",
  text: "#1f2021",
  muted: "#222324",
  shadow: "#252627",
  nodeSelected: "#28292a",
  nodeActive: "#2b2c2d",
  unknownHeader: "#2e2f30",
  unknownSocket: "#313233",
  linkMuted: "#343536",
  knifeMuted: "#373839",
  emphasis: "#3a3b3c",
  focus: "#3d3e3f",
  editOutline: "#404142",
  resize: "#434445",
  muteOverlay: "#464748",
  boxSelectionFill: "#494a4b",
  checkerLight: "#4c4d4e",
  checkerDark: "#4f5051",
  widgetBorder: "#525354",
  rampBorder: "#555657",
  resourceBackground: "#58595a",
} as const;
const ramp = () => ({
  kind: "json" as const,
  value: {
    colorMode: "rgb",
    interpolation: "linear",
    hueInterpolation: "near",
    stops: [
      { id: "dark", position: 0, color: [0, 0, 0, 1] },
      { id: "light", position: 1, color: [1, 1, 1, 1] },
    ],
  },
});
const input = <const T extends string>(type: T, title = "Input", visible = true) => ({
  title,
  direction: "input" as const,
  type,
  maxIncomingLinks: 1,
  visible,
  value: null,
  showValue: false,
});
const output = <const T extends string>(type: T, title = "Output") => ({
  title,
  direction: "output" as const,
  type,
  maxIncomingLinks: 0,
  visible: true,
  value: null,
  showValue: false,
});
const base = (behavior: "standard" | "frame" | "reroute", style: "violet" | "frameStyle" = "violet") => ({
  version: 1,
  title: "Node",
  behavior,
  style,
  parameters: {},
  sockets: {},
  ui: [],
  muteBypass: [],
  migrations: [],
});
const compositionSource = {
  schemaVersion: 2,
  id: "adversarial-presentation",
  version: 73,
  compatibility: { wildcardInputTypes: ["universal-destination"] },
  theme,
  socketTypes: {
    "source-signal": { title: "Signal", color: "#a1b2c3", acceptsFrom: ["source-signal"] },
    "universal-destination": { title: "Everything", color: "#d4e5f6", acceptsFrom: [] },
  },
  nodeStyles: { violet: { header: "#7654ab" }, frameStyle: { header: "#abcdef" } },
  resources: {
    photo: {
      kind: "image",
      title: "Default image title",
      openTitle: "Default open",
      accept: ["image/png"],
      referencePrefix: "test-image:",
      maxBytes: 1024,
      maxWidth: 64,
      maxHeight: 64,
      maxPixels: 4096,
    },
  },
  nodes: {
    "fxnode.common.frame": { ...base("standard"), title: "Frame-looking ordinary" },
    "fxnode.common.reroute": { ...base("standard"), title: "Reroute-looking ordinary" },
    "totally-unrelated-container": { ...base("frame", "frameStyle"), title: "Actual frame" },
    "odd-junction": {
      ...base("reroute"),
      title: "Actual reroute",
      sockets: { in: input("source-signal"), out: output("source-signal") },
      ui: [
        { kind: "socket", socket: "in" },
        { kind: "socket", socket: "out" },
      ],
      muteBypass: [["in", "out"]],
    },
    source: { ...base("standard"), sockets: { out: output("source-signal") }, ui: [{ kind: "socket", socket: "out" }] },
    destination: {
      ...base("standard"),
      parameters: { mode: { type: "string", default: { kind: "string", value: "show" }, enum: ["show", "hide"] } },
      sockets: {
        wild: input("universal-destination", "Schema label"),
        hidden: input("source-signal", "Hidden socket"),
        conditional: input("source-signal", "Conditional socket"),
      },
      ui: [
        { kind: "parameter", parameter: "mode" },
        { kind: "socket", socket: "wild", title: "Overridden socket title" },
        { kind: "hidden", target: "socket", socket: "hidden" },
        { kind: "socket", socket: "conditional", visibleWhen: { parameter: "mode", equals: "show" } },
      ],
    },
    controls: {
      ...base("standard"),
      parameters: {
        ramp: { type: "json", codec: "color-ramp/v1", default: ramp() },
        ordinaryRampJson: { type: "json", codec: "color-ramp/v1", default: ramp() },
        imageRef: { type: "string", default: { kind: "string", value: "" } },
        lift: { type: "number", default: { kind: "number", value: 0 } },
        gamma: { type: "number", default: { kind: "number", value: 1 } },
        gain: { type: "number", default: { kind: "number", value: 1 } },
        liftColor: { type: "color", default: { kind: "color", value: [0, 0, 0, 1] } },
        gammaColor: { type: "color", default: { kind: "color", value: [0.5, 0.5, 0.5, 1] } },
        gainColor: { type: "color", default: { kind: "color", value: [1, 1, 1, 1] } },
      },
      sockets: {},
      ui: [
        { kind: "widget", widget: "color-ramp", parameter: "ramp" },
        { kind: "parameter", parameter: "ordinaryRampJson", title: "Plain JSON" },
        { kind: "resource", resource: "photo", parameter: "imageRef", title: "Plate", openTitle: "Choose plate" },
        {
          kind: "widget",
          widget: "grading-wheels",
          bindings: [
            { title: "Shadows", scalar: "lift", color: "liftColor" },
            { title: "Midtones", scalar: "gamma", color: "gammaColor" },
            { title: "Highlights", scalar: "gain", color: "gainColor" },
          ],
        },
      ],
    },
  },
} as const;
const compiled = compileFxNodeComposition(structuredClone(compositionSource));
const runtime = bindFxNodeHeadless(compiled);
const transform = { center: { x: 200, y: 0 }, zoom: 1, viewport: { x: 1600, y: 1000 }, dpr: 1 };
const link = (
  id: string,
  fromNodeId: string,
  fromSocketId: string,
  toNodeId: string,
  toSocketId: string,
  muted = false,
) => ({ id, fromNodeId, fromSocketId, toNodeId, toSocketId, muted, extensions: {} });

test("custom behavior metadata, UI placement, widgets, resources, and colors are authoritative", () => {
  const lookFrame = runtime.materializeNode("look-frame", "fxnode.common.frame", { x: 0, y: 0 });
  const lookReroute = runtime.materializeNode("look-reroute", "fxnode.common.reroute", { x: 220, y: 0 });
  const frame = runtime.materializeNode("container", "totally-unrelated-container", { x: 0, y: 300 });
  const child = runtime.materializeNode("child", "source", { x: 40, y: -40 }, "container");
  const junction = runtime.materializeNode("junction", "odd-junction", { x: 400, y: 100 });
  const controls = runtime.materializeNode("arbitrary", "controls", { x: 600, y: 300 });
  const destination = runtime.materializeNode("dest", "destination", { x: 1100, y: 200 });
  const document = {
    ...runtime.emptyDocument("custom"),
    nodes: {
      "look-frame": lookFrame,
      "look-reroute": lookReroute,
      container: frame,
      child,
      junction,
      arbitrary: controls,
      dest: destination,
    },
    links: {},
  } as any;
  const layout = layoutGraph(compiled, document, transform);
  assert.equal(layout.nodes.get("look-frame" as never)?.kind, "node");
  assert.equal(layout.nodes.get("look-reroute" as never)?.kind, "node");
  assert.equal(layout.nodes.get("container" as never)?.kind, "frame");
  assert.equal(layout.nodes.get("junction" as never)?.kind, "reroute");
  assert.equal(layout.drawOrder[0], "container");
  const f = layout.nodes.get("container" as never)!,
    c = layout.nodes.get("child" as never)!;
  assert.ok(f.bounds.width > 100 && f.bounds.x <= c.bounds.x - 30, "custom frame fits its child and paints behind it");
  assert.equal(layout.sockets.has("dest:hidden" as never), false);
  assert.equal(layout.sockets.get("dest:wild" as never)?.label, "Overridden socket title");
  assert.equal(layout.sockets.has("dest:conditional" as never), true);
  const hiddenMode = {
    ...destination,
    parameters: { ...destination.parameters, mode: { kind: "string", value: "hide" } },
  };
  const conditional = layoutGraph(compiled, { ...document, nodes: { ...document.nodes, dest: hiddenMode } }, transform);
  assert.equal(conditional.sockets.has("dest:conditional" as never), false);
  assert.equal(
    conditional.nodes.get("dest" as never)?.rows.some((r: any) => r.socketId === "dest:conditional"),
    false,
  );
  const explicit = layout.controls.get("arbitrary:parameter:ramp")!,
    plain = layout.controls.get("arbitrary:parameter:ordinaryRampJson")!,
    resource = layout.controls.get("arbitrary:parameter:imageRef")!;
  assert.equal(explicit.kind, "color-ramp");
  assert.ok(explicit.rampBounds);
  assert.equal(plain.kind, "readonly-json");
  assert.equal(plain.rampBounds, undefined);
  assert.equal(resource.kind, "resource");
  assert.ok(resource.resourceBounds);
  assert.equal(resource.label, "Plate");
  assert.equal(resource.openTitle, "Choose plate");
  const wheels = layout.nodes.get("arbitrary" as never)?.rows.find((r) => r.kind === "grading-wheels");
  assert.ok(wheels?.kind === "grading-wheels");
  assert.deepEqual(
    wheels.wheels.map((w) => w.label),
    ["Shadows", "Midtones", "Highlights"],
  );
  assert.ok(wheels.wheels.every((w) => layout.controls.get(w.colorControlId)?.colorWheelBounds));
  assert.equal(layout.nodes.get("arbitrary" as never)?.headerColor, "#7654ab");
  assert.equal(layout.sockets.get("junction:out" as never)?.color, "#a1b2c3");
  const unknown = {
    ...junction,
    known: false,
    typeId: "odd-junction",
    id: "opaque",
    sockets: junction.sockets.map((s) => ({ ...s, id: `opaque:${s.key}` })),
  };
  const opaqueLayout = layoutGraph(compiled, { ...document, nodes: { opaque: unknown }, links: {} }, transform);
  assert.equal(opaqueLayout.nodes.get("opaque" as never)?.kind, "node");
  assert.equal(opaqueLayout.nodes.get("opaque" as never)?.headerColor, theme.unknownHeader);
  assert.ok([...opaqueLayout.sockets.values()].every((s) => s.color === theme.unknownSocket));
});

test("custom wildcard compatibility agrees across layout, interaction, and bound runtime", () => {
  const source = runtime.materializeNode("s", "source"),
    destination = runtime.materializeNode("d", "destination", { x: 300, y: 0 }),
    document = { ...runtime.emptyDocument("compat"), nodes: { s: source, d: destination }, links: {} } as any,
    layout = layoutGraph(compiled, document, transform),
    from = layout.sockets.get("s:out" as never)!,
    to = layout.sockets.get("d:wild" as never)!;
  assert.equal(to.dataType, "universal-destination");
  assert.equal(to.wildcardInput, true);
  assert.equal(layoutSocketsCompatible(from, to), true);
  assert.equal(
    compatibleTargets(layout, from.id).some((x) => x.id === to.id),
    true,
  );
  assert.equal(runtime.socketsCompatible(source.sockets[0]!, destination.sockets[0]!), true);
});

test("custom reroute mute propagation excludes opaque lookalikes and survives indexed rebuild authority", () => {
  const source = runtime.materializeNode("s", "source"),
    reroute = runtime.materializeNode("r", "odd-junction"),
    destination = runtime.materializeNode("d", "destination"),
    unknown = {
      ...reroute,
      known: false,
      id: "u",
      typeId: "odd-junction",
      sockets: reroute.sockets.map((s) => ({ ...s, id: `u:${s.key}` })),
    };
  const links = {
    a: link("a", "s", "s:out", "r", "r:in", true),
    b: link("b", "r", "r:out", "d", "d:wild"),
    c: link("c", "s", "s:out", "u", "u:in", true),
    d: link("d", "u", "u:out", "d", "d:wild"),
  };
  const document = {
      ...runtime.emptyDocument("mute"),
      nodes: { s: source, r: reroute, d: destination, u: unknown },
      links,
    } as any,
    muted = effectivelyMutedLinks(compiled, document);
  assert.deepEqual([...muted].sort(), ["a", "b", "c"]);
  const store = new IndexedLayoutStore(compiled, document),
    authority = store.compiled;
  store.rebuild({ ...document, links: {} });
  assert.equal(store.compiled, authority);
  assert.equal(store.compiled, compiled);
  assert.equal(store.scene.nodes.get("r" as never)?.kind, "reroute");
});
