import { compileFxNodeComposition } from "@lib/composition/index.js";
import { bindFxNodeHeadless } from "@lib/headless-runtime.js";
import { layoutGraph } from "@lib/layout/layout-graph.js";
import { worldToView } from "@lib/layout/geometry.js";
import { renderCanvas } from "@lib/render/canvas-renderer.js";
import { linkId } from "@lib/core/types.js";

const theme = {
  background: "#010203",
  grid: "#010203",
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
  shadow: "#00000000",
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
const base = () =>
  ({
    version: 1,
    behavior: "standard",
    style: "custom",
    parameters: {},
    muteBypass: [],
    migrations: [],
  }) as const;
const composition = {
  schemaVersion: 2,
  id: "browser-render-proof",
  version: 1,
  compatibility: { wildcardInputTypes: [] },
  theme,
  socketTypes: { signal: { title: "Signal", color: "#a1b2c3", acceptsFrom: ["signal"] } },
  nodeStyles: { custom: { header: "#7654ab" } },
  resources: {},
  nodes: {
    source: {
      ...base(),
      title: "Source",
      sockets: {
        out: {
          title: "Out",
          direction: "output",
          type: "signal",
          maxIncomingLinks: 0,
          visible: true,
          value: null,
          showValue: false,
        },
      },
      ui: [{ kind: "socket", socket: "out" }],
    },
    target: {
      ...base(),
      title: "Target",
      sockets: {
        in: {
          title: "In",
          direction: "input",
          type: "signal",
          maxIncomingLinks: 1,
          visible: true,
          value: null,
          showValue: false,
        },
      },
      ui: [{ kind: "socket", socket: "in" }],
    },
  },
} as const;
const compiled = compileFxNodeComposition(composition),
  runtime = bindFxNodeHeadless(compiled);
const source = runtime.materializeNode("source", "source", { x: -200, y: 80 }),
  target = runtime.materializeNode("target", "target", { x: 100, y: 80 });
const link = {
  id: linkId("wire"),
  fromNodeId: source.id,
  fromSocketId: source.sockets[0]!.id,
  toNodeId: target.id,
  toSocketId: target.sockets[0]!.id,
  muted: false,
  extensions: {},
};
const graph = { ...runtime.emptyDocument("render"), nodes: { source, target }, links: { wire: link } };
const transform = { center: { x: 0, y: 0 }, zoom: 1, viewport: { x: 500, y: 300 }, dpr: 1 },
  layout = layoutGraph(compiled, graph, transform);
const canvas = document.querySelector<HTMLCanvasElement>("#presentation")!,
  context = canvas.getContext("2d")!;
renderCanvas(context as unknown as OffscreenCanvasRenderingContext2D, layout, compiled.theme);
const pixel = (x: number, y: number) => Array.from(context.getImageData(Math.round(x), Math.round(y), 1, 1).data);
const contains = (x: number, y: number, rgba: readonly number[]) => {
  const data = context.getImageData(Math.round(x) - 3, Math.round(y) - 3, 7, 7).data;
  for (let i = 0; i < data.length; i += 4) if (rgba.every((v, j) => data[i + j] === v)) return true;
  return false;
};
const node = layout.nodes.get(source.id)!,
  socket = layout.sockets.get(source.sockets[0]!.id)!,
  wire = layout.links.get("wire" as never)!,
  header = worldToView({ x: node.bounds.x + 60, y: node.bounds.y - 12 }, transform),
  socketPoint = worldToView(socket.anchor, transform),
  middle = worldToView(wire.points[Math.floor(wire.points.length / 2)]!, transform);
const result = {
  background: pixel(3, 3),
  header: pixel(header.x, header.y),
  socket: pixel(socketPoint.x, socketPoint.y),
  linkContainsCustomColor: contains(middle.x, middle.y, [161, 178, 195, 255]),
};
(window as unknown as { presentationResult: typeof result }).presentationResult = result;
