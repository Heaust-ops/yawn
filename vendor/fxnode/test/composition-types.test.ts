import assert from "node:assert/strict";
import test from "node:test";
import type {
  createFxNode as CreateFxNode,
  FxNode,
  FxNodeView,
  FxNodeViewOptions,
  FxNodeCompositionSeed,
  NodeParameterId,
  NodeSocketId,
  NodeStyleId,
  NodeTypeId,
  ResourceId,
  SocketTypeId,
  Themed,
  RemovedNode,
  RemovedSocket,
} from "@lib/index.js";
import { compileFxNodeComposition } from "@lib/composition/compile.js";
import {
  composeNode,
  composeSocket,
  removeNode,
  removeSocket,
  setHeaderStyles,
  setTheme,
} from "@lib/composition/compose.js";
import { APPLICATION_ID, APPLICATION_RESOURCES, APPLICATION_VERSION } from "../examples/blender/nodes/application.js";
import type {
  createApplicationFxNode as CreateApplicationFxNode,
  ApplicationFxNode,
} from "../examples/blender/application-browser.js";

type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Expect<T extends true> = T;
const theme = {
  background: "#000",
  grid: "#000",
  frame: "#000",
  frameHeader: "#000",
  body: "#000",
  control: "#000",
  controlFill: "#000",
  controlEditing: "#000",
  textSelection: "#000",
  outline: "#000",
  text: "#000",
  muted: "#000",
  shadow: "#000",
  nodeSelected: "#000",
  nodeActive: "#000",
  unknownHeader: "#000",
  unknownSocket: "#000",
  linkMuted: "#000",
  knifeMuted: "#000",
  emphasis: "#000",
  focus: "#000",
  editOutline: "#000",
  resize: "#000",
  muteOverlay: "#000",
  boxSelectionFill: "#000",
  checkerLight: "#000",
  checkerDark: "#000",
  widgetBorder: "#000",
  rampBorder: "#000",
  resourceBackground: "#000",
} as const;
const valid = {
  schemaVersion: 2,
  id: "types",
  version: 1,
  compatibility: { wildcardInputTypes: [] },
  theme,
  socketTypes: { number: { title: "Number", color: "#abc", acceptsFrom: ["number"] } },
  nodeStyles: { basic: { header: "#abc" } },
  resources: {
    preview: {
      kind: "image",
      title: "Preview",
      openTitle: "Open",
      accept: ["image/png"],
      referencePrefix: "test-image:",
      maxBytes: 100_000_000,
      maxWidth: 10_000,
      maxHeight: 3,
      maxPixels: 100_000_000,
    },
  },
  nodes: {
    math: {
      version: 2,
      title: "Math",
      behavior: "standard",
      style: "basic",
      parameters: {
        amount: { type: "number", default: { kind: "number", value: 1 }, precision: 2 },
        ramp: { type: "json", codec: "color-ramp/v1", default: { kind: "json", value: null } },
      },
      sockets: {
        input: {
          title: "In",
          direction: "input",
          type: "number",
          maxIncomingLinks: 1,
          visible: true,
          value: null,
          showValue: false,
        },
        output: {
          title: "Out",
          direction: "output",
          type: "number",
          maxIncomingLinks: 0,
          visible: true,
          value: null,
          showValue: false,
        },
      },
      ui: [
        { kind: "parameter", parameter: "amount", visibleWhen: { all: [{ parameter: "amount", equals: 1 }] } },
        { kind: "widget", widget: "color-ramp", parameter: "ramp" },
        { kind: "resource", resource: "preview", parameter: "amount" },
      ],
      muteBypass: [["input", "output"]],
      migrations: [
        {
          fromVersion: 1,
          toVersion: 2,
          steps: [
            { kind: "materialize-missing", target: "parameter", key: "amount" },
            { kind: "rename-socket", from: "old", to: "input" },
          ],
        },
      ],
    },
    other: {
      version: 1,
      title: "Other",
      behavior: "standard",
      style: "basic",
      parameters: { label: { type: "string", default: { kind: "string", value: "" } } },
      sockets: {
        result: {
          title: "Result",
          direction: "output",
          type: "number",
          maxIncomingLinks: 0,
          visible: true,
          value: null,
          showValue: false,
        },
      },
      ui: [
        { kind: "parameter", parameter: "label" },
        { kind: "socket", socket: "result" },
      ],
      muteBypass: [],
      migrations: [],
    },
  },
} as const;
type _N = Expect<Equal<NodeTypeId<typeof valid>, "math" | "other">>;
type _T = Expect<Equal<SocketTypeId<typeof valid>, "number">>;
type _S = Expect<Equal<NodeStyleId<typeof valid>, "basic">>;
type _R = Expect<Equal<ResourceId<typeof valid>, "preview">>;
type _P = Expect<Equal<NodeParameterId<typeof valid, "math">, "amount" | "ramp">>;
type _K = Expect<Equal<NodeSocketId<typeof valid, "math">, "input" | "output">>;
type _OP = Expect<Equal<NodeParameterId<typeof valid, "other">, "label">>;
type _OK = Expect<Equal<NodeSocketId<typeof valid, "other">, "result">>;
if (false) {
  const createFxNode = null as unknown as typeof CreateFxNode;
  const compiled = compileFxNodeComposition(valid);
  const restyled = setHeaderStyles(valid, { alternate: { header: "#123456" } });
  type _HeaderStyle = Expect<Equal<NodeStyleId<typeof restyled>, "alternate">>;
  void (null as _HeaderStyle | null);
  const typeId = compiled.nodes.get("math")?.typeId;
  type _CN = Expect<Equal<Exclude<typeof typeId, undefined>, "math" | "other">>;
  void (null as _CN | null);
  const requestedBytes = compiled.source.resources.preview.maxBytes;
  type _RequestedBytes = Expect<Equal<typeof requestedBytes, 100_000_000>>;
  void (null as _RequestedBytes | null);
  const effectiveBytes = compiled.resources.get("preview")!.maxBytes;
  type _EffectiveBytes = Expect<Equal<typeof effectiveBytes, number>>;
  void (null as _EffectiveBytes | null);
  // @ts-expect-error exact compiled node keys
  compiled.nodes.get("unknown");
  const canvas = null as unknown as HTMLCanvasElement;
  const viewport = { width: 1, height: 1, dpr: 1 };
  type _ApplicationApi = Expect<Equal<Awaited<ReturnType<typeof CreateApplicationFxNode>>, ApplicationFxNode>>;
  void (null as _ApplicationApi | null);
  const options = {
    applicationId: APPLICATION_ID,
    applicationVersion: APPLICATION_VERSION,
    resources: APPLICATION_RESOURCES,
  };
  const nodeless = createFxNode(options);
  void nodeless;
  // @ts-expect-error application identity, version, and resources are mandatory
  void createFxNode({});
  // @ts-expect-error constructor composition was removed
  void createFxNode({ ...options, composition: valid });
  // @ts-expect-error constructor graph layout was removed
  void createFxNode({ ...options, layout: {} });
  // @ts-expect-error canvases belong to attached views, not root construction
  void createFxNode({ ...options, canvas });
  // @ts-expect-error viewports belong to attached views, not root construction
  void createFxNode({ ...options, viewport });
  const apiPromise = createFxNode(options);
  type _Api = Expect<Equal<Awaited<typeof apiPromise>, FxNode>>;
  void (null as _Api | null);
  void apiPromise.then((api) => {
    const viewOptions: FxNodeViewOptions = {
      canvas,
      viewport,
      initialCamera: { center: { x: 0, y: 0 }, zoom: 1 },
    };
    api.attachView(viewOptions).then((view) => {
      type _View = Expect<Equal<typeof view, FxNodeView>>;
      void (null as _View | null);
      view.feedInput({ kind: "focus", phase: "focus" });
      view.setViewport(viewport);
      view.getHostSnapshot().selection.nodeCount;
      const unsubscribeHost = view.subscribeHost(() => view.getHostSnapshot());
      const unsubscribeRequests = view.onHostRequests((request) => void request);
      void view.addNode({ typeId: "math", viewPosition: { x: 0, y: 0 } });
      void view.removeSelected();
      void view.setSelectedMuted(true);
      void view.provideResource(
        { viewId: view.id, token: "token", graphVersion: 0, compositionRevision: 0 },
        { name: "image.png", mime: "image/png", bytes: new ArrayBuffer(1) },
      );
      unsubscribeHost();
      unsubscribeRequests();
      void view.whenRendered();
      void view.detach();
      view.id.length;
      // @ts-expect-error a view ID cannot be replaced by consumers
      view.id = "replacement";
      // @ts-expect-error graph authority remains on the root
      view.dispatch({ type: "undo" });
      // @ts-expect-error graph snapshots remain on the root
      view.getState();
      // @ts-expect-error composition authority remains on the root
      view.setTheme(theme);
      // @ts-expect-error composition authority remains on the root
      view.composeNode("other", valid.nodes.other);
    });
    // @ts-expect-error canvas is required
    api.attachView({ viewport });
    // @ts-expect-error viewport is required
    api.attachView({ canvas });
    // @ts-expect-error view IDs are internal
    api.attachView({ canvas, viewport, viewId: "caller-owned" });
    // @ts-expect-error view-only operation
    api.feedInput({ kind: "focus", phase: "focus" });
    // @ts-expect-error old copy presentation API was removed
    api.copyTo(canvas);
    // @ts-expect-error old mirror presentation API was removed
    api.addMirror(canvas);
    // @ts-expect-error old mirror presentation API was removed
    api.removeMirror(canvas);
    // @ts-expect-error rendering barriers belong to views
    api.whenRendered();
    api.setCompatibility({ wildcardInputTypes: ["any"] });
    api.dispatch({ type: "node.add", nodeType: "math", position: { x: 0, y: 0 } });
    api.getState().then((snapshot) => {
      const known = snapshot.nodes.find((node) => node.known);
      if (known) {
        type _Known = Expect<Equal<typeof known.typeId, string>>;
        void (null as _Known | null);
      }
    });
    api.onSnapshots((event) => {
      const known = event.snapshot.nodes.find((node) => node.known);
      if (known) {
        type _Known = Expect<Equal<typeof known.typeId, string>>;
        void (null as _Known | null);
      }
    });
    api.onMutations((event) => {
      for (const mutation of event.mutations)
        if (mutation.kind === "node.set" && mutation.after?.known) {
          type _Known = Expect<Equal<typeof mutation.after.typeId, string>>;
          void (null as _Known | null);
        }
    });
    // @ts-expect-error live composition validates definition-local parameter references
    void api.composeNode("bad-live-node", { ...valid.nodes.other, ui: [{ kind: "parameter", parameter: "missing" }] });
    void api
      .composeSocket("dynamic", { title: "Dynamic", color: "#abcdef", acceptsFrom: ["dynamic"] } as const)
      .then(async (receipt) => {
        await api.composeNode("dynamic-node", {
          version: 1,
          title: "Dynamic Node",
          behavior: "standard",
          style: "basic",
          parameters: {},
          sockets: {
            out: {
              title: "Out",
              direction: "output",
              type: "dynamic",
              maxIncomingLinks: 0,
              visible: true,
              value: null,
              showValue: false,
            },
          },
          ui: [{ kind: "socket", socket: "out" }],
          muteBypass: [],
          migrations: [],
        });
        api.dispatch({ type: "node.add", nodeType: "dynamic-node", position: { x: 0, y: 0 } });
        await api.removeNode("dynamic-node");
        void receipt.revision;
      });
    api.onCompositionChanges((event) => {
      event.change.kind;
      event.revision;
      event.graphVersion;
      // @ts-expect-error composition events never expose definitions
      event.change.definition;
    });
  });
}
test("public composition definition preserves identity", () => assert.equal(valid.id, "types"));

const seed = {
  schemaVersion: 2,
  id: "composed-types",
  version: 1,
  compatibility: { wildcardInputTypes: [] },
  nodeStyles: { basic: { header: "#abc" } },
  resources: {},
  socketTypes: {},
  nodes: {},
} as const satisfies FxNodeCompositionSeed;
const themed = setTheme(seed, theme);
const scalar = composeSocket(themed, "scalar", { title: "Scalar", color: "#abc", acceptsFrom: ["scalar"] });
const vector = composeSocket(scalar, "vector", { title: "Vector", color: "#def", acceptsFrom: ["vector", "scalar"] });
const composed = composeNode(vector, "constant", {
  version: 1,
  title: "Constant",
  behavior: "standard",
  style: "basic",
  parameters: { value: { type: "number", default: { kind: "number", value: 1 } } },
  sockets: {
    out: {
      title: "Out",
      direction: "output",
      type: "scalar",
      maxIncomingLinks: 0,
      visible: true,
      value: null,
      showValue: false,
    },
  },
  ui: [
    { kind: "parameter", parameter: "value" },
    { kind: "socket", socket: "out" },
  ],
  muteBypass: [],
  migrations: [],
});
const overridden = composeNode(composed, "constant", {
  ...composed.nodes.constant,
  title: "Replacement",
  sockets: {
    result: {
      title: "Result",
      direction: "output",
      type: "vector",
      maxIncomingLinks: 0,
      visible: true,
      value: null,
      showValue: false,
    },
  },
  ui: [
    { kind: "parameter", parameter: "value" },
    { kind: "socket", socket: "result" },
  ],
  muteBypass: [],
});
const noNode = removeNode(overridden, "constant"),
  noVector = removeSocket(vector, "vector");
type _ComposedSockets = Expect<Equal<SocketTypeId<typeof vector>, "scalar" | "vector">>;
type _ComposedNodes = Expect<Equal<NodeTypeId<typeof composed>, "constant">>;
type _OverriddenSocket = Expect<Equal<NodeSocketId<typeof overridden, "constant">, "result">>;
type _RemovedNode = Expect<Equal<NodeTypeId<typeof noNode>, never>>;
type _RemovedSocket = Expect<Equal<SocketTypeId<typeof noVector>, "scalar">>;
type _ThemedExport = Expect<Equal<typeof themed, Themed<typeof seed, typeof theme>>>;
type _RemovedNodeExport = Expect<Equal<typeof noNode, RemovedNode<typeof overridden, "constant">>>;
type _RemovedSocketExport = Expect<Equal<typeof noVector, RemovedSocket<typeof vector, "vector">>>;
void (null as
  | _ComposedSockets
  | _ComposedNodes
  | _OverriddenSocket
  | _RemovedNode
  | _RemovedSocket
  | _ThemedExport
  | _RemovedNodeExport
  | _RemovedSocketExport
  | null);
if (false) {
  // @ts-expect-error acceptsFrom may only reference itself or a composed socket type
  composeSocket(scalar, "bad", { title: "Bad", color: "#000", acceptsFrom: ["missing"] });
  // @ts-expect-error removeNode only accepts a currently composed node ID
  removeNode(composed, "missing");
  // @ts-expect-error removeSocket only accepts a currently composed socket ID
  removeSocket(vector, "missing");
  // @ts-expect-error composeNode validates style references against the current composition
  composeNode(vector, "bad", { ...composed.nodes.constant, style: "missing" });
  composeNode(vector, "bad-size", {
    ...composed.nodes.constant,
    // @ts-expect-error node dimensions are calculated internally
    defaultSize: { x: 1, y: 1 },
  });
  compileFxNodeComposition(composed);

  // Every representable reference and finite schema discriminator is checked below.
  compileFxNodeComposition({
    ...valid,
    socketTypes: {
      number: {
        ...valid.socketTypes.number,
        // @ts-expect-error unknown acceptsFrom socket type
        acceptsFrom: ["missing"],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown node socket type
        sockets: { ...valid.nodes.math.sockets, input: { ...valid.nodes.math.sockets.input, type: "missing" } },
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown style
        style: "missing",
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown resource
        ui: [{ kind: "resource", resource: "missing", parameter: "amount" }],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown UI parameter
        ui: [{ kind: "parameter", parameter: "missing" }],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown UI socket
        ui: [{ kind: "socket", socket: "missing" }],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown nested visibility parameter
        ui: [{ kind: "parameter", parameter: "amount", visibleWhen: { any: [{ parameter: "missing", equals: 1 }] } }],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown grading scalar/color references
        ui: [
          {
            kind: "widget",
            widget: "grading-wheels",
            bindings: [
              { title: "A", scalar: "missing", color: "amount" },
              { title: "B", scalar: "amount", color: "amount" },
              { title: "C", scalar: "amount", color: "amount" },
            ],
          },
        ],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown mute bypass socket
        muteBypass: [["missing", "output"]],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown migration current-side parameter
        migrations: [
          { fromVersion: 1, toVersion: 2, steps: [{ kind: "rename-parameter", from: "old", to: "missing" }] },
        ],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unknown migration current-side socket
        migrations: [
          { fromVersion: 1, toVersion: 2, steps: [{ kind: "materialize-missing", target: "socket", key: "missing" }] },
        ],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        // @ts-expect-error unsupported widget
        ui: [{ kind: "widget", widget: "dial", parameter: "amount" }],
      },
    },
  });
  compileFxNodeComposition({
    ...valid,
    nodes: {
      math: {
        ...valid.nodes.math,
        migrations: [
          {
            fromVersion: 1,
            toVersion: 2,
            steps: [
              {
                kind: "migrate-parameter",
                parameter: "amount",
                // @ts-expect-error unsupported migration codec
                codec: "unknown",
              },
            ],
          },
        ],
      },
    },
  });
}
