import { createFxNode, type FxNode, type FxNodeCompositionData, type FxNodeView } from "@lib/index.js";
import { prepareFxNodeBrowserHost, type PreparedFxNodeBrowserHost } from "../../examples/shared/browser-host.js";
import type { FxNodeResourceOpenRequest } from "@lib/index.js";
import type { GraphState } from "@lib/core/types.js";

const color = "#202124" as const;
const theme = {
  background: color,
  grid: color,
  frame: color,
  frameHeader: color,
  body: "#35383e",
  control: color,
  controlFill: color,
  controlEditing: color,
  textSelection: color,
  outline: "#101114",
  text: "#eeeeee",
  muted: "#999999",
  shadow: "#000000",
  nodeSelected: "#ed5700",
  nodeActive: "#ed5700",
  unknownHeader: "#555555",
  unknownSocket: "#777777",
  linkMuted: "#d94b4b",
  knifeMuted: "#e85b5b",
  emphasis: "#ffffff",
  focus: "#f5a623",
  editOutline: "#666a70",
  resize: "#8b8e95",
  muteOverlay: "#14141459",
  boxSelectionFill: "#f5a6231f",
  checkerLight: "#aaaaaa",
  checkerDark: "#777777",
  widgetBorder: "#111216",
  rampBorder: "#111111",
  resourceBackground: "#202228",
} as const;
const composition = (
  id: string,
  version: number,
  nodeType: string,
  title: string,
  header: `#${string}`,
  maxBytes: number,
  maxWidth: number,
  accept: string,
) =>
  ({
    schemaVersion: 2,
    id,
    version,
    compatibility: { wildcardInputTypes: [] },
    theme,
    socketTypes: { signal: { title: "Signal", color: header, acceptsFrom: ["signal"] } },
    nodeStyles: { application: { header } },
    resources: {
      image: {
        kind: "image",
        title: "Image",
        openTitle: "Open",
        accept: [accept],
        referencePrefix: `${id}-image:`,
        maxBytes,
        maxWidth,
        maxHeight: 4,
        maxPixels: maxWidth * 4,
      },
      strictImage: {
        kind: "image",
        title: "Strict Image",
        openTitle: "Open",
        accept: [accept],
        referencePrefix: `${id}-image:`,
        maxBytes,
        maxWidth: 1,
        maxHeight: 1,
        maxPixels: 1,
      },
    },
    nodes: {
      [nodeType]: {
        version: 1,
        title,
        behavior: "standard",
        style: "application",
        parameters: { image: { type: "string", default: { kind: "string", value: "" } } },
        sockets: {
          output: {
            title: "Output",
            direction: "output",
            type: "signal",
            maxIncomingLinks: 0,
            visible: true,
            value: null,
            showValue: false,
          },
        },
        ui: [
          { kind: "resource", resource: "image", parameter: "image" },
          { kind: "socket", socket: "output" },
        ],
        muteBypass: [],
        migrations: [],
      },
      [`${nodeType}-strict`]: {
        version: 1,
        title: `${title} Strict`,
        behavior: "standard",
        style: "application",
        parameters: { image: { type: "string", default: { kind: "string", value: "" } } },
        sockets: {
          output: {
            title: "Output",
            direction: "output",
            type: "signal",
            maxIncomingLinks: 0,
            visible: true,
            value: null,
            showValue: false,
          },
        },
        ui: [
          { kind: "resource", resource: "strictImage", parameter: "image" },
          { kind: "socket", socket: "output" },
        ],
        muteBypass: [],
        migrations: [],
      },
    },
  }) as unknown as FxNodeCompositionData;

export const compositionA = composition(
  "application-a",
  11,
  "alpha-node",
  "Alpha Node",
  "#9b59b6",
  512,
  1,
  "image/png",
);
export const compositionB = composition(
  "application-b",
  22,
  "beta-node",
  "Beta Node",
  "#2e86de",
  2048,
  4,
  "image/webp",
);
export const migrationComposition = {
  ...compositionA,
  id: "worker-migration-custom",
  version: 73,
  nodes: {
    "odd-migrator": {
      ...compositionA.nodes["alpha-node"]!,
      version: 2,
      parameters: { level: { type: "number", default: { kind: "number", value: 5 } } },
      sockets: {
        source: {
          title: "Source",
          direction: "output",
          type: "signal",
          maxIncomingLinks: 0,
          visible: true,
          value: null,
          showValue: false,
        },
        sink: {
          title: "Sink",
          direction: "input",
          type: "signal",
          maxIncomingLinks: 4,
          visible: true,
          value: null,
          showValue: false,
        },
      },
      ui: [
        { kind: "parameter", parameter: "level" },
        { kind: "socket", socket: "source" },
        { kind: "socket", socket: "sink" },
      ],
      migrations: [
        {
          fromVersion: 1,
          toVersion: 2,
          steps: [
            { kind: "rename-parameter", from: "oldLevel", to: "level" },
            { kind: "rename-socket", from: "oldSource", to: "source" },
            { kind: "rename-socket", from: "oldSink", to: "sink" },
          ],
        },
      ],
    },
  },
} as unknown as FxNodeCompositionData;
const layout = (id: string, version: number) => ({
  schemaVersion: 2 as const,
  graphId: id,
  catalogVersion: version,
  nodes: [],
  links: [],
  metadata: {},
});
const migrationLayout = {
  schemaVersion: 2 as const,
  graphId: "worker-historical",
  catalogVersion: 1,
  nodes: [
    {
      id: "left",
      typeId: "odd-migrator",
      typeVersion: 1,
      position: { x: 0, y: 0 },
      size: { x: 180, y: 190 },
      label: "",
      parameters: { oldLevel: { kind: "number", value: 9 } },
      sockets: [
        {
          id: "left:oldSource",
          key: "oldSource",
          label: "Source",
          direction: "output",
          dataType: "signal",
          accepts: [],
          maxIncomingLinks: 0,
          visible: true,
        },
        {
          id: "left:oldSink",
          key: "oldSink",
          label: "Sink",
          direction: "input",
          dataType: "signal",
          accepts: ["signal"],
          maxIncomingLinks: 4,
          visible: true,
        },
      ],
      muted: false,
      collapsed: false,
      extensions: {},
    },
    {
      id: "right",
      typeId: "odd-migrator",
      typeVersion: 1,
      position: { x: 300, y: 0 },
      size: { x: 180, y: 190 },
      label: "",
      parameters: { oldLevel: { kind: "number", value: 3 } },
      sockets: [
        {
          id: "right:oldSource",
          key: "oldSource",
          label: "Source",
          direction: "output",
          dataType: "signal",
          accepts: [],
          maxIncomingLinks: 0,
          visible: true,
        },
        {
          id: "right:oldSink",
          key: "oldSink",
          label: "Sink",
          direction: "input",
          dataType: "signal",
          accepts: ["signal"],
          maxIncomingLinks: 4,
          visible: true,
        },
      ],
      muted: false,
      collapsed: false,
      extensions: {},
    },
  ],
  links: [
    {
      id: "historical-link",
      fromNodeId: "left",
      fromSocketId: "left:oldSource",
      toNodeId: "right",
      toSocketId: "right:oldSink",
      muted: false,
      extensions: { kept: true },
    },
  ],
  metadata: { fixture: "migration" },
};
const hosts = new WeakMap<FxNode, PreparedFxNodeBrowserHost>(),
  views = new WeakMap<FxNode, FxNodeView>();
const create = async <C extends FxNodeCompositionData>(
  canvas: HTMLCanvasElement,
  value: C,
  initialLayout: unknown = layout(value.id, value.version),
  activateResourcePicker?: (request: FxNodeResourceOpenRequest) => void,
) => {
  const host = prepareFxNodeBrowserHost({
    canvas,
    addNodeMenuTemplate: document.querySelector<HTMLTemplateElement>("#add-node-menu-template")!,
    ...(activateResourcePicker ? { activateResourcePicker } : {}),
  });
  let api: FxNode | undefined;
  const { id, version, resources } = value;
  try {
    api = await createFxNode({
      applicationId: id,
      applicationVersion: version,
      resources,
    });
    await api.loadComposition(value as never, { expectedRevision: 0 });
    if (initialLayout === migrationLayout) await api.load(initialLayout);
    const view = await api.attachView({ canvas, viewport: host.initialViewport });
    host.attach(api, view);
    hosts.set(api, host);
    views.set(api, view);
    return api;
  } catch (error) {
    host.destroy();
    api?.destroy();
    throw error;
  }
};
const destroy = (api: FxNode) => {
  hosts.get(api)?.destroy();
  hosts.delete(api);
  views.delete(api);
  api.destroy();
};

type Harness = {
  readonly compositionA: typeof compositionA;
  readonly compositionB: typeof compositionB;
  createA(activateResourcePicker?: (request: FxNodeResourceOpenRequest) => void): Promise<FxNode>;
  createB(activateResourcePicker?: (request: FxNodeResourceOpenRequest) => void): Promise<FxNode>;
  createMigration(): Promise<FxNode>;
  createRaw(canvas: HTMLCanvasElement, value: FxNodeCompositionData): Promise<FxNode>;
  view(api: FxNode): FxNodeView;
  destroy(api: FxNode): void;
};
(window as unknown as { workerCompositionTest: Harness }).workerCompositionTest = {
  compositionA,
  compositionB,
  createA: (activate) =>
    create(document.querySelector<HTMLCanvasElement>("#application-a")!, compositionA, undefined, activate),
  createB: (activate) =>
    create(document.querySelector<HTMLCanvasElement>("#application-b")!, compositionB, undefined, activate),
  createMigration: () =>
    create(document.querySelector<HTMLCanvasElement>("#application-a")!, migrationComposition, migrationLayout),
  createRaw: async (canvas, value) => {
    const api = await create(canvas, compositionA);
    try {
      await api.loadComposition(value, { expectedRevision: 1 });
      return api;
    } catch (error) {
      destroy(api);
      throw error;
    }
  },
  view: (api) => {
    const view = views.get(api);
    if (!view) throw new Error("FxNode test view is unavailable");
    return view;
  },
  destroy,
};
