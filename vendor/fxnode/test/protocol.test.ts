import assert from "node:assert/strict";
import test from "node:test";
import {
  PROTOCOL_VERSION,
  validCommandReceipt,
  validCompositionReceipt,
  validRequest,
  validWorkerMessage,
} from "@lib/browser/protocol.js";
import {
  decodeFxNodeActionOptions,
  decodeFxNodeAddNodeParams,
  decodeFxNodeCamera,
  decodeFxNodeInput,
  decodeFxNodeResourceAuthorization,
  decodeFxNodeResourceData,
  decodeFxNodeViewport,
} from "@lib/browser/host-decode.js";
import { fxNodeDevicePixels } from "@lib/browser/view-limits.js";

const viewport = { width: 1, height: 1, dpr: 1 };
const viewId = "internal-view";

test("protocol v2 messages are rejected", () => {
  assert.equal(validRequest({ protocol: 2, type: "state.get", id: "request" }), false);
  assert.equal(
    validWorkerMessage({ protocol: 2, type: "view.node-menu.result", viewId, requestId: "request", open: false }),
    false,
  );
});

test("old unprefixed view message names are rejected", () => {
  for (const type of [
    "viewport",
    "input",
    "node.add",
    "selection.remove",
    "selection.mute",
    "resource.set",
    "pointer.flush",
    "frame.consumed",
  ])
    assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type, viewId }), false);
  for (const type of ["frame", "selection.host", "node-menu.result", "resource.open"])
    assert.equal(validWorkerMessage({ protocol: PROTOCOL_VERSION, type, viewId }), false);
});

test("view protocol is internally scoped and camera decoding is hostile-safe", () => {
  const attach = {
    protocol: PROTOCOL_VERSION,
    type: "view.attach",
    id: "request",
    viewId: "internal-view",
    viewport,
    camera: { center: { x: 3, y: -4 }, zoom: 2 },
  };
  assert.equal(validRequest(attach), true);
  assert.equal(validRequest({ ...attach, viewId: undefined }), false);
  assert.equal(validRequest({ ...attach, camera: undefined }), false);
  assert.equal(validRequest({ ...attach, callerViewId: "public" }), false);
  for (const id of [undefined, "", 1, "x".repeat(513)]) assert.equal(validRequest({ ...attach, viewId: id }), false);
  assert.equal(validRequest({ ...attach, extra: true }), false);
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "view.detach", id: "request", viewId }), true);
  assert.equal(
    validRequest({ protocol: PROTOCOL_VERSION, type: "view.detach", id: "request", viewId, extra: true }),
    false,
  );
  assert.deepEqual(decodeFxNodeCamera(attach.camera), attach.camera);
  const frozen = Object.freeze({ center: Object.freeze({ x: 3, y: -4 }), zoom: 0.1 });
  assert.deepEqual(decodeFxNodeCamera(frozen), frozen);
  assert.deepEqual(decodeFxNodeCamera({ center: { x: 0, y: 0 }, zoom: 4 }), {
    center: { x: 0, y: 0 },
    zoom: 4,
  });
  for (const value of [NaN, Infinity, -Infinity]) {
    assert.throws(() => decodeFxNodeCamera({ center: { x: value, y: 0 }, zoom: 1 }), TypeError);
    assert.throws(() => decodeFxNodeCamera({ center: { x: 0, y: 0 }, zoom: value }), TypeError);
  }
  assert.throws(
    () => decodeFxNodeCamera({ center: { x: 0, y: 0 }, zoom: 1, extra: true } as never),
    new TypeError("Invalid FxNode camera"),
  );
  assert.throws(
    () => decodeFxNodeCamera({ center: { x: 0, y: 0 }, zoom: 1, [Symbol()]: true } as never),
    new TypeError("Invalid FxNode camera"),
  );
  let cameraReads = 0;
  const accessorCamera = Object.defineProperty({ center: { x: 0, y: 0 } }, "zoom", {
    enumerable: true,
    get() {
      cameraReads++;
      return 1;
    },
  });
  assert.throws(() => decodeFxNodeCamera(accessorCamera as never), new TypeError("Invalid FxNode camera"));
  assert.equal(cameraReads, 0);
  const center = { x: 7, y: 8 };
  const detachedCamera = decodeFxNodeCamera({ center, zoom: 1 });
  center.x = 99;
  assert.deepEqual(detachedCamera, { center: { x: 7, y: 8 }, zoom: 1 });
  for (const zoom of [0.099, 4.001])
    assert.throws(() => decodeFxNodeCamera({ center: { x: 0, y: 0 }, zoom }), RangeError);
  const nested = Proxy.revocable({ x: 0, y: 0 }, {});
  nested.revoke();
  assert.throws(() => decodeFxNodeCamera({ center: nested.proxy, zoom: 1 }), new TypeError("Invalid FxNode camera"));
  const revoked = Proxy.revocable({}, {});
  revoked.revoke();
  assert.throws(() => decodeFxNodeCamera(revoked.proxy as never), new TypeError("Invalid FxNode camera"));
});

test("view.frame worker messages are exact and view IDs are bounded", () => {
  const previous = globalThis.ImageBitmap;
  class TestImageBitmap {
    readonly width = 1;
    readonly height = 1;
  }
  Object.defineProperty(globalThis, "ImageBitmap", { configurable: true, value: TestImageBitmap });
  try {
    const frame = {
      protocol: PROTOCOL_VERSION,
      type: "view.frame",
      viewId,
      bitmap: new TestImageBitmap(),
      renderId: 0,
      frameId: 0,
      hostGeneration: 0,
      surfaceGeneration: 0,
      deviceWidth: 1,
      deviceHeight: 1,
      host: { colorPickerOpen: false },
    };
    assert.equal(validWorkerMessage(frame), true);
    for (const candidate of [
      { ...frame, bitmap: {} },
      { ...frame, frameId: -1 },
      { ...frame, renderId: 0.5 },
      { ...frame, hostGeneration: Number.MAX_SAFE_INTEGER + 1 },
      { ...frame, surfaceGeneration: -1 },
      { ...frame, deviceWidth: 0 },
      { ...frame, deviceHeight: 8193 },
      { ...frame, host: { colorPickerOpen: false, extra: true } },
      { ...frame, extra: true },
    ])
      assert.equal(validWorkerMessage(candidate), false);
    for (const invalidViewId of [undefined, "", 1, "x".repeat(513)]) {
      const candidate = { ...frame, viewId: invalidViewId };
      if (invalidViewId === undefined) delete (candidate as { viewId?: unknown }).viewId;
      assert.equal(validWorkerMessage(candidate), false);
    }
  } finally {
    if (previous === undefined) delete (globalThis as { ImageBitmap?: unknown }).ImageBitmap;
    else Object.defineProperty(globalThis, "ImageBitmap", { configurable: true, value: previous });
  }
});

test("input and viewport decoder tables cover every variant and boundary", () => {
  const m0 = { alt: false, control: false, meta: false, shift: false },
    m15 = { alt: true, control: true, meta: true, shift: true };
  const cases = [
    [
      { kind: "focus", phase: "focus" },
      { kind: "focus", phase: "focus" },
    ],
    [
      { kind: "outside-pointer", button: -1 },
      { kind: "outside-pointer", button: -1 },
    ],
    [
      {
        kind: "pointer",
        phase: "move",
        pointerId: 1,
        pointerType: "pen",
        position: { x: -2, y: -3 },
        button: -1,
        buttons: 0,
        modifiers: m0,
      },
      {
        kind: "pointer",
        phase: "move",
        pointerId: 1,
        pointerType: "pen",
        position: { x: -2, y: -3 },
        button: -1,
        buttons: 0,
        modifiers: 0,
      },
    ],
    [
      { kind: "wheel", position: { x: -1, y: -2 }, delta: { x: -3, y: 4 }, modifiers: m15 },
      { kind: "wheel", position: { x: -1, y: -2 }, delta: { x: -3, y: 4 }, modifiers: 15 },
    ],
    [
      { kind: "key", phase: "up", key: "A", code: "KeyA", repeat: false, modifiers: m15 },
      { kind: "key", phase: "up", key: "A", code: "KeyA", repeat: false, modifiers: 15 },
    ],
  ] as const;
  for (const [input, expected] of cases) assert.deepEqual(decodeFxNodeInput(input as never), expected);
  for (const value of [
    { width: 0, height: 8192, dpr: Number.MIN_VALUE },
    { width: 8192, height: 0, dpr: 1 },
    { width: 4096, height: 4096, dpr: 1 },
  ])
    assert.deepEqual(decodeFxNodeViewport(value), value);
  assert.deepEqual(decodeFxNodeViewport({ width: 0, height: 0, dpr: 4 }), { width: 0, height: 0, dpr: 4 });
  assert.deepEqual(fxNodeDevicePixels(0, 0, 1), { width: 1, height: 1 });
  assert.deepEqual(fxNodeDevicePixels(0.49, 0.5, 1), { width: 1, height: 1 });
  assert.deepEqual(fxNodeDevicePixels(1.5, 2.5, 1), { width: 2, height: 3 });
  assert.equal(fxNodeDevicePixels(4097, 4096, 1), undefined);
  for (const value of [
    { width: -1, height: 1, dpr: 1 },
    { width: 8193, height: 0, dpr: 1 },
    { width: 4097, height: 4096, dpr: 1 },
    { width: 1, height: 1, dpr: 0 },
  ])
    assert.throws(() => decodeFxNodeViewport(value), RangeError);
});

test("protocol tables reject malformed pointer fences", () => {
  const base = { protocol: PROTOCOL_VERSION, type: "view.pointer.flush", viewId },
    move = {
      kind: "pointer",
      phase: "move",
      pointerId: 1,
      pointerType: "mouse",
      position: { x: 0, y: 0 },
      button: 0,
      buttons: 0,
      modifiers: 0,
    };
  assert.equal(
    validRequest({ ...base, pointerFence: { generation: 0, before: { sequence: 1, hostGeneration: 2, event: move } } }),
    true,
  );
  for (const pointerFence of [
    {},
    { generation: 0.5 },
    { generation: 0, extra: true },
    { generation: 0, before: {} },
    { generation: 0, before: { sequence: 1, event: move } },
    { generation: 0, before: { sequence: 1, hostGeneration: -1, event: move } },
    { generation: 0, before: { sequence: 1, hostGeneration: 0, event: { ...move, phase: "down" } } },
    { generation: 0, before: { sequence: 1, hostGeneration: 0, event: { ...move, position: { x: Infinity, y: 0 } } } },
  ])
    assert.equal(validRequest({ ...base, pointerFence }), false);
});
test("host generations are required, exact, and nonnegative", () => {
  const event = { kind: "focus", phase: "focus" },
    input = { protocol: PROTOCOL_VERSION, type: "view.input", viewId, event, hostGeneration: 0 },
    viewportRequest = {
      protocol: PROTOCOL_VERSION,
      type: "view.viewport",
      id: "resize",
      viewId,
      viewport,
      expectedSurfaceGeneration: 1,
      hostGeneration: 0,
    };
  for (const value of [input, viewportRequest]) assert.equal(validRequest(value), true);
  for (const generation of [undefined, -1, 0.5]) {
    const candidate = { ...input, ...(generation === undefined ? {} : { hostGeneration: generation }) };
    if (generation === undefined) delete (candidate as { hostGeneration?: unknown }).hostGeneration;
    assert.equal(validRequest(candidate), false);
  }
  assert.equal(validRequest({ ...input, extra: true }), false);
  assert.equal(validRequest({ ...viewportRequest, hostGeneration: -1 }), false);
  assert.equal(validRequest({ ...viewportRequest, pointerFence: { generation: 1 } }), true);
  assert.equal(validRequest({ ...viewportRequest, pointerFence: { generation: 0.5 } }), false);
  for (const expectedSurfaceGeneration of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1])
    assert.equal(validRequest({ ...viewportRequest, expectedSurfaceGeneration }), false);
  const render = { protocol: PROTOCOL_VERSION, type: "view.render", viewId, renderId: 1, hostGeneration: 0 };
  assert.equal(validRequest(render), true);
  for (const renderId of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1])
    assert.equal(validRequest({ ...render, renderId }), false);
  const consumed = { protocol: PROTOCOL_VERSION, type: "view.frame.consumed", viewId, frameId: 0 };
  assert.equal(validRequest(consumed), true);
  assert.equal(validRequest({ ...consumed, extra: true }), false);
  for (const frameId of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1])
    assert.equal(validRequest({ ...consumed, frameId }), false);
});
test("resource open correlation is exact on input and worker messages", () => {
  const event = {
      kind: "pointer",
      phase: "down",
      pointerId: 1,
      pointerType: "mouse",
      position: { x: 2, y: 3 },
      button: 0,
      buttons: 1,
      modifiers: 0,
    },
    input = {
      protocol: PROTOCOL_VERSION,
      type: "view.input",
      viewId,
      event,
      hostGeneration: 0,
      resourceOpenRequestId: "open-1",
    },
    resource = {
      id: "image",
      kind: "image",
      title: "Image",
      openTitle: "Open image",
      accept: ["image/png"],
      maxBytes: 1024,
      maxWidth: 4,
      maxHeight: 4,
      maxPixels: 16,
    },
    open = {
      protocol: PROTOCOL_VERSION,
      type: "view.resource.open",
      viewId,
      requestId: "open-1",
      authorization: { viewId, token: "node:parameter:image", graphVersion: 2, compositionRevision: 3 },
      resource,
    };
  assert.equal(validRequest(input), true);
  assert.equal(validWorkerMessage(open), true);
  assert.equal(validWorkerMessage({ ...open, authorization: { ...open.authorization, viewId: "other" } }), true);
  assert.equal(validWorkerMessage({ ...open, authorization: { ...open.authorization, extra: true } }), false);
  assert.equal(validWorkerMessage({ ...open, authorization: { ...open.authorization, viewId: "" } }), false);
  for (const requestId of ["", "x".repeat(513), 1]) {
    assert.equal(validRequest({ ...input, resourceOpenRequestId: requestId }), false);
    assert.equal(validWorkerMessage({ ...open, requestId }), false);
  }
  assert.equal(validRequest({ ...input, extra: true }), false);
  assert.equal(validWorkerMessage({ ...open, bounds: { x: 0, y: 0, width: 1, height: 1 } }), false);
  assert.equal(validWorkerMessage({ ...open, authorization: { ...open.authorization, extra: true } }), false);
  assert.equal(validWorkerMessage({ ...open, resource: { ...resource, accept: ["image/png"], extra: true } }), false);
});
test("public host values decode once without getters and detach nested input", () => {
  let reads = 0;
  const getter = Object.defineProperty({ kind: "focus" }, "phase", {
    enumerable: true,
    get() {
      reads++;
      return "focus";
    },
  });
  assert.throws(() => decodeFxNodeInput(getter as never), new TypeError("Invalid FxNode input"));
  assert.equal(reads, 0);
  const position = { x: 1, y: 2 },
    mods = { alt: true, control: false, meta: true, shift: false },
    input = {
      kind: "pointer",
      phase: "down",
      pointerId: 1,
      pointerType: "mouse",
      position,
      button: 0,
      buttons: 1,
      modifiers: mods,
    } as const,
    wire = decodeFxNodeInput(input);
  position.x = 9;
  mods.control = true;
  assert.deepEqual(wire, {
    kind: "pointer",
    phase: "down",
    pointerId: 1,
    pointerType: "mouse",
    position: { x: 1, y: 2 },
    button: 0,
    buttons: 1,
    modifiers: 5,
  });
  assert.throws(
    () => decodeFxNodeInput({ ...input, [Symbol()]: true } as never),
    new TypeError("Invalid FxNode input"),
  );
  assert.throws(
    () => decodeFxNodeViewport({ width: Infinity, height: 1, dpr: 1 }),
    new TypeError("Invalid FxNode viewport"),
  );
  assert.throws(
    () => decodeFxNodeViewport({ width: 8192, height: 8192, dpr: 1 }),
    new RangeError("FxNode viewport is outside supported bounds"),
  );
});

test("public action values reject accessors, symbols, and hostile proxies", () => {
  assert.deepEqual(decodeFxNodeActionOptions({}), { kind: "current" });
  let reads = 0;
  const options = Object.defineProperty({}, "expectedVersion", {
    enumerable: true,
    get() {
      reads++;
      return 1;
    },
  });
  const params = Object.defineProperty({ typeId: "test", viewPosition: { x: 1, y: 2 } }, "nodeId", {
    enumerable: true,
    get() {
      reads++;
      return "node";
    },
  });
  assert.throws(() => decodeFxNodeActionOptions(options), new TypeError("Invalid action options"));
  assert.throws(() => decodeFxNodeAddNodeParams(params), new TypeError("Invalid add-node parameters"));
  assert.equal(reads, 0);
  assert.throws(
    () => decodeFxNodeAddNodeParams({ typeId: "test", viewPosition: { x: 1, y: 2 }, [Symbol()]: true }),
    new TypeError("Invalid add-node parameters"),
  );
  const revoked = Proxy.revocable({}, {});
  revoked.revoke();
  assert.throws(() => decodeFxNodeActionOptions(revoked.proxy), new TypeError("Invalid action options"));
  assert.throws(() => decodeFxNodeAddNodeParams(revoked.proxy), new TypeError("Invalid add-node parameters"));
});
test("protocol validators are total and bound request and node type ids", () => {
  const throwing = new Proxy(
    {},
    {
      get() {
        throw new Error("hostile");
      },
      ownKeys() {
        throw new Error("hostile");
      },
    },
  );
  const target = {};
  const revoked = Proxy.revocable(target, {});
  revoked.revoke();
  for (const value of [throwing, revoked.proxy]) {
    assert.doesNotThrow(() => validRequest(value));
    assert.equal(validRequest(value), false);
    assert.doesNotThrow(() => validWorkerMessage(value));
    assert.equal(validWorkerMessage(value), false);
  }
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "state.get", id: "x".repeat(513) }), false);
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "node.add-at-view",
      id: "r",
      nodeId: "n",
      nodeType: "x".repeat(129),
      viewPosition: { x: 0, y: 0 },
    }),
    false,
  );
  // The live command wire format retains its historical 512-code-unit node type bound.
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "command",
      id: "r",
      expected: { kind: "current" },
      command: { type: "node.add", nodeId: "n", nodeType: "x".repeat(129), position: { x: 0, y: 0 } },
    }),
    true,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "command",
      id: "r",
      expected: { kind: "current" },
      command: { type: "node.add", nodeId: "n", nodeType: "x".repeat(513), position: { x: 0, y: 0 } },
    }),
    false,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "command",
      id: "r",
      expected: { kind: "current" },
      command: Object.create({ type: "undo" }),
    }),
    false,
  );
  assert.equal(
    validRequest({ protocol: PROTOCOL_VERSION, type: "load", id: "r", data: {}, expected: { kind: "current" } }),
    true,
  );
  assert.equal(
    validRequest({ protocol: PROTOCOL_VERSION, type: "load", id: "r", layout: {}, expected: { kind: "current" } }),
    false,
  );
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "save.data", id: "r" }), true);
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "save.data", id: "r", extra: true }), false);
  assert.equal(validRequest(Object.create({ protocol: PROTOCOL_VERSION, type: "state.get", id: "r" })), false);
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "snapshot", id: "r" }), false);
  assert.equal(validRequest({ protocol: PROTOCOL_VERSION, type: "state.get", id: "r" }), true);
  let inspected = 0;
  const opaque = Object.defineProperty({}, "nodes", {
    enumerable: true,
    get() {
      inspected++;
      throw new Error("payload inspected");
    },
  });
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "state.set",
      id: "r",
      state: opaque,
      expected: { kind: "current" },
    }),
    true,
  );
  assert.equal(inspected, 0);
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "state.set",
      id: "r",
      state: {},
      expected: { kind: "current" },
      extra: true,
    }),
    false,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "view.resource.set",
      viewId,
      id: "r",
      authorization: { viewId, token: "t", graphVersion: 2, compositionRevision: 0 },
      resource: { name: "x.png", mime: "image/png", bytes: new ArrayBuffer(1) },
      expected: { kind: "current" },
    }),
    true,
  );
  assert.equal(
    validRequest(
      Object.assign(Object.create({ event: { kind: "focus", phase: "focus" } }), {
        protocol: PROTOCOL_VERSION,
        type: "view.input",
        viewId,
      }),
    ),
    false,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "view.pointer.flush",
      viewId,
      pointerFence: Object.create({ generation: 0 }),
    }),
    false,
  );
});

test("action requests, selection projections, menu results, and receipts are exact", () => {
  const current = { kind: "current" as const };
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "view.node.add",
      viewId,
      id: "r",
      nodeId: "n",
      typeId: "t",
      viewPosition: { x: 1, y: 2 },
      expected: current,
    }),
    true,
  );
  assert.equal(
    validRequest({ protocol: PROTOCOL_VERSION, type: "view.selection.remove", viewId, id: "r", expected: current }),
    true,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "view.selection.remove",
      viewId,
      id: "r",
      expected: current,
      value: false,
    }),
    false,
  );
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "view.selection.mute",
      viewId,
      id: "r",
      expected: current,
      value: false,
    }),
    true,
  );
  assert.equal(
    validRequest({ protocol: PROTOCOL_VERSION, type: "view.selection.mute", viewId, id: "r", expected: current }),
    false,
  );
  const selection = (nodeCount: number, linkCount: number, canRemove: boolean) => ({
    protocol: PROTOCOL_VERSION,
    type: "view.selection.host",
    viewId,
    projection: { nodeCount, linkCount, canRemove, mute: { enabled: false } },
  });
  assert.equal(validWorkerMessage(selection(0, 0, false)), true);
  assert.equal(validWorkerMessage(selection(1, 0, true)), true);
  assert.equal(validWorkerMessage(selection(0, 0, true)), false);
  assert.equal(validWorkerMessage(selection(1, 0, false)), false);
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "view.node-menu.result",
      viewId,
      requestId: "r",
      open: false,
    }),
    true,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "view.node-menu.result",
      viewId,
      requestId: "r",
      open: true,
      compositionRevision: 0,
      viewPosition: { x: 1, y: 2 },
    }),
    true,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "view.node-menu.result",
      viewId,
      requestId: "r",
      open: false,
      viewPosition: { x: 1, y: 2 },
    }),
    false,
  );
  assert.equal(validCommandReceipt({ status: "noop", version: 0 }), true);
  assert.equal(validCommandReceipt({ status: "committed", version: 1 }), true);
  assert.equal(validCommandReceipt({ status: "noop", version: 0, extra: true }), false);
});

test("resource DTO and wire decoding is exact, bounded, and hostile-safe", () => {
  const authorization = { viewId, token: "control", graphVersion: 2, compositionRevision: 3 },
    bytes = new ArrayBuffer(4),
    resource = { name: "pixel.png", mime: "image/png", bytes };
  assert.deepEqual(decodeFxNodeResourceAuthorization(authorization), authorization);
  assert.throws(
    () => decodeFxNodeResourceAuthorization({ ...authorization, viewId: "" }),
    new TypeError("Invalid resource authorization"),
  );
  assert.throws(
    () => decodeFxNodeResourceAuthorization({ ...authorization, extra: true }),
    new TypeError("Invalid resource authorization"),
  );
  assert.equal(decodeFxNodeResourceData(resource).bytes, bytes);
  let reads = 0;
  const accessor = Object.defineProperty({ token: "control", graphVersion: 2 }, "compositionRevision", {
    enumerable: true,
    get() {
      reads++;
      return 3;
    },
  });
  assert.throws(() => decodeFxNodeResourceAuthorization(accessor), new TypeError("Invalid resource authorization"));
  assert.equal(reads, 0);
  assert.throws(
    () => decodeFxNodeResourceData({ ...resource, [Symbol()]: true }),
    new TypeError("Invalid resource data"),
  );
  assert.throws(
    () => decodeFxNodeResourceData({ ...resource, bytes: new ArrayBuffer(0) }),
    new RangeError("Resource data is outside supported bounds"),
  );
  const request = {
    protocol: PROTOCOL_VERSION,
    type: "view.resource.set",
    viewId,
    id: "r",
    authorization,
    resource,
    expected: { kind: "exact", version: 2 },
    pointerFence: { generation: 1 },
  };
  assert.equal(validRequest(request), true);
  assert.equal(validRequest({ ...request, authorization: { ...authorization, extra: true } }), false);
  assert.equal(validRequest({ ...request, resource: { ...resource, name: "bad\nname" } }), false);
  assert.equal(validRequest({ ...request, expected: { kind: "exact", version: -1 } }), false);
  const nested = Proxy.revocable({ x: 1, y: 2 }, {});
  nested.revoke();
  assert.throws(
    () => decodeFxNodeAddNodeParams({ typeId: "test", viewPosition: nested.proxy }),
    new TypeError("Invalid add-node parameters"),
  );
  const bytesProxy = Proxy.revocable(new ArrayBuffer(4), {});
  bytesProxy.revoke();
  assert.throws(
    () => decodeFxNodeResourceData({ ...resource, bytes: bytesProxy.proxy }),
    new TypeError("Invalid resource data"),
  );
});

test("malformed composition init remains recognizable protocol", () => {
  const init = {
    protocol: PROTOCOL_VERSION,
    type: "init",
    id: "init",
    applicationId: "app",
    applicationVersion: 1,
    resources: {},
    historyLimit: 0,
  };
  assert.equal(validRequest(init), true);
  for (const presentation of [
    { viewport },
    { camera: { center: { x: 0, y: 0 }, zoom: 1 } },
    { pointerLane: new SharedArrayBuffer(32) },
  ])
    assert.equal(validRequest({ ...init, ...presentation }), false);
  assert.equal(
    validRequest({
      protocol: PROTOCOL_VERSION,
      type: "init",
      id: "init",
      composition: {},
      layout: null,
      historyLimit: 0,
    }),
    false,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "response",
      id: "init",
      ok: false,
      error: {
        code: "composition.invalid",
        message: "Invalid composition",
        issues: [{ code: "composition.schema", path: "/", message: "invalid" }],
      },
    }),
    true,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "response",
      id: "init",
      ok: false,
      error: { code: "link.endpoint", message: "Missing endpoint", path: "/links/stale" },
    }),
    true,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "response",
      id: "init",
      ok: false,
      error: { code: "link.endpoint", message: "Missing endpoint", path: "/" + "x".repeat(513) },
    }),
    false,
  );
  assert.equal(
    validWorkerMessage(Object.create({ protocol: PROTOCOL_VERSION, type: "response", id: "init", ok: true })),
    false,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "response",
      id: "init",
      ok: false,
      error: Object.create({ code: "worker.error", message: "inherited" }),
    }),
    false,
  );
});

test("live composition protocol validates updates, revisions, receipts and events", () => {
  const request = (update: unknown, expected: unknown = { kind: "current" }) => ({
    protocol: PROTOCOL_VERSION,
    type: "composition.update",
    id: "request",
    expected,
    update,
  });
  for (const update of [
    { kind: "composition.load", composition: {} },
    { kind: "theme.set", theme: {} },
    { kind: "compatibility.set", compatibility: { wildcardInputTypes: [] } },
    { kind: "socket.compose", id: "signal", definition: {} },
    { kind: "socket.remove", id: "signal" },
    { kind: "node.compose", id: "source", definition: {} },
    { kind: "node.remove", id: "source" },
  ])
    assert.equal(validRequest(request(update)), true);
  assert.equal(validRequest(request({ kind: "composition.load", composition: {}, extra: true })), false);
  for (const expected of [
    { kind: "exact", revision: -1 },
    { kind: "exact", revision: 0.5 },
    { kind: "exact", revision: Number.MAX_SAFE_INTEGER + 1 },
  ])
    assert.equal(validRequest(request({ kind: "node.remove", id: "source" }, expected)), false);
  for (const id of ["", "x".repeat(129), "bad\nvalue", "__proto__", "prototype", "constructor"])
    assert.equal(validRequest(request({ kind: "node.remove", id })), false);
  assert.equal(validRequest({ ...request({ kind: "node.remove", id: "source" }), extra: true }), false);
  assert.equal(validRequest(request({ kind: "node.remove", id: "source", extra: true })), false);
  assert.equal(
    validCompositionReceipt({
      status: "committed",
      revision: 1,
      graphVersion: 0,
      graphChanged: false,
      historyReset: true,
    }),
    true,
  );
  assert.equal(
    validCompositionReceipt({ status: "noop", revision: 0, graphVersion: 0, graphChanged: false, historyReset: false }),
    true,
  );
  assert.equal(
    validCompositionReceipt(
      Object.create({ status: "noop", revision: 0, graphVersion: 0, graphChanged: false, historyReset: false }),
    ),
    false,
  );
  assert.equal(
    validCompositionReceipt({ status: "noop", revision: 0, graphVersion: 0, graphChanged: true, historyReset: false }),
    false,
  );
  assert.equal(
    validCompositionReceipt({
      status: "committed",
      revision: 1,
      graphVersion: 0,
      graphChanged: false,
      historyReset: false,
    }),
    false,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "composition.event",
      envelope: {
        baseRevision: 0,
        revision: 1,
        change: { kind: "node.compose", id: "source" },
        baseGraphVersion: 2,
        graphVersion: 3,
        graphChanged: true,
        historyReset: true,
      },
    }),
    true,
  );
  assert.equal(
    validWorkerMessage({
      protocol: PROTOCOL_VERSION,
      type: "composition.event",
      envelope: {
        baseRevision: 0,
        revision: 2,
        change: { kind: "theme.set" },
        baseGraphVersion: 2,
        graphVersion: 2,
        graphChanged: false,
        historyReset: true,
      },
    }),
    false,
  );
});
