import type {
  AddNodeParams,
  FxNodeInput,
  FxNodeResourceAuthorization,
  FxNodeResourceData,
  FxNodeCamera,
  FxNodeViewport,
} from "./host-types.js";
import { fxNodeDevicePixels, FXNODE_VIEW_LIMITS } from "./view-limits.js";
import type { InputEventWire, VersionExpectation } from "./protocol.js";

type Values = Record<string, unknown>;
function inspect(value: unknown): { values: Values; keys: readonly string[] } | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return;
  const own = Reflect.ownKeys(value);
  if (own.some((key) => typeof key !== "string")) return;
  const result: Values = {};
  for (const key of own as string[]) {
    const descriptor = Reflect.getOwnPropertyDescriptor(value, key);
    if (!descriptor || !("value" in descriptor)) return;
    result[key] = descriptor.value;
  }
  return { values: result, keys: own as string[] };
}
const exact = (
  item: { values: Values; keys: readonly string[] } | undefined,
  keys: readonly string[],
): Values | undefined =>
  item && item.keys.length === keys.length && item.keys.every((key) => keys.includes(key)) ? item.values : undefined;
const data = (value: unknown, keys: readonly string[]) => exact(inspect(value), keys);
const point = (value: unknown): { x: number; y: number } | undefined => {
  const v = data(value, ["x", "y"]);
  return v && typeof v.x === "number" && Number.isFinite(v.x) && typeof v.y === "number" && Number.isFinite(v.y)
    ? { x: v.x, y: v.y }
    : undefined;
};
const modifiers = (value: unknown): number | undefined => {
  const v = data(value, ["alt", "control", "meta", "shift"]);
  if (
    !v ||
    typeof v.alt !== "boolean" ||
    typeof v.control !== "boolean" ||
    typeof v.meta !== "boolean" ||
    typeof v.shift !== "boolean"
  )
    return;
  return (v.alt ? 1 : 0) | (v.control ? 2 : 0) | (v.meta ? 4 : 0) | (v.shift ? 8 : 0);
};
export function decodeFxNodeInput(value: FxNodeInput): InputEventWire {
  try {
    const item = inspect(value as unknown),
      kind = item?.values.kind;
    const base = kind === "focus" ? exact(item, ["kind", "phase"]) : undefined;
    if (base && (base.phase === "focus" || base.phase === "blur")) return { kind: "focus", phase: base.phase };
    const outside = kind === "outside-pointer" ? exact(item, ["kind", "button"]) : undefined;
    if (outside && Number.isInteger(outside.button))
      return { kind: "outside-pointer", button: outside.button as number };
    const pointer =
        kind === "pointer"
          ? exact(item, ["kind", "phase", "pointerId", "pointerType", "position", "button", "buttons", "modifiers"])
          : undefined,
      p = pointer && point(pointer.position),
      m = pointer && modifiers(pointer.modifiers);
    if (
      pointer &&
      ["down", "move", "up", "cancel"].includes(String(pointer.phase)) &&
      Number.isSafeInteger(pointer.pointerId) &&
      typeof pointer.pointerType === "string" &&
      pointer.pointerType.length <= 64 &&
      p &&
      Number.isInteger(pointer.button) &&
      Number.isInteger(pointer.buttons) &&
      m !== undefined
    )
      return {
        kind: "pointer",
        phase: pointer.phase as "down" | "move" | "up" | "cancel",
        pointerId: pointer.pointerId as number,
        pointerType: pointer.pointerType,
        position: p,
        button: pointer.button as number,
        buttons: pointer.buttons as number,
        modifiers: m,
      };
    const wheel = kind === "wheel" ? exact(item, ["kind", "position", "delta", "modifiers"]) : undefined,
      wp = wheel && point(wheel.position),
      delta = wheel && point(wheel.delta),
      wm = wheel && modifiers(wheel.modifiers);
    if (wheel && wp && delta && wm !== undefined) return { kind: "wheel", position: wp, delta, modifiers: wm };
    const key = kind === "key" ? exact(item, ["kind", "phase", "key", "code", "repeat", "modifiers"]) : undefined,
      km = key && modifiers(key.modifiers);
    if (
      key &&
      (key.phase === "down" || key.phase === "up") &&
      typeof key.key === "string" &&
      key.key.length <= 256 &&
      typeof key.code === "string" &&
      key.code.length <= 256 &&
      typeof key.repeat === "boolean" &&
      km !== undefined
    )
      return { kind: "key", phase: key.phase, key: key.key, code: key.code, repeat: key.repeat, modifiers: km };
  } catch {}
  throw new TypeError("Invalid FxNode input");
}
export function decodeFxNodeViewport(value: FxNodeViewport): FxNodeViewport {
  let v: Values | undefined;
  try {
    v = data(value as unknown, ["width", "height", "dpr"]);
  } catch {}
  if (
    !v ||
    typeof v.width !== "number" ||
    !Number.isFinite(v.width) ||
    typeof v.height !== "number" ||
    !Number.isFinite(v.height) ||
    typeof v.dpr !== "number" ||
    !Number.isFinite(v.dpr)
  )
    throw new TypeError("Invalid FxNode viewport");
  if (
    v.width < 0 ||
    v.width > FXNODE_VIEW_LIMITS.maxLogicalDimension ||
    v.height < 0 ||
    v.height > FXNODE_VIEW_LIMITS.maxLogicalDimension ||
    v.dpr <= 0 ||
    v.dpr > FXNODE_VIEW_LIMITS.maxDpr ||
    !fxNodeDevicePixels(v.width, v.height, v.dpr)
  )
    throw new RangeError("FxNode viewport is outside supported bounds");
  return Object.freeze({ width: v.width, height: v.height, dpr: v.dpr });
}

/** Decodes and detaches a camera from an exact, data-only hostile object. */
export function decodeFxNodeCamera(value: FxNodeCamera): FxNodeCamera {
  let v: Values | undefined, center: { x: number; y: number } | undefined;
  try {
    v = data(value as unknown, ["center", "zoom"]);
    center = v && point(v.center);
  } catch {}
  if (!v || !center || typeof v.zoom !== "number" || !Number.isFinite(v.zoom))
    throw new TypeError("Invalid FxNode camera");
  if (v.zoom < FXNODE_VIEW_LIMITS.minZoom || v.zoom > FXNODE_VIEW_LIMITS.maxZoom)
    throw new RangeError("FxNode camera is outside supported bounds");
  return Object.freeze({ center: Object.freeze(center), zoom: v.zoom });
}

export function decodeFxNodeActionOptions(value: unknown): VersionExpectation {
  if (value === undefined) return { kind: "current" };
  let item: ReturnType<typeof inspect> | undefined;
  try {
    item = inspect(value);
  } catch {}
  if (item?.keys.length === 0) return { kind: "current" };
  const v = exact(item, ["expectedVersion"]);
  if (!v || !Number.isSafeInteger(v.expectedVersion) || (v.expectedVersion as number) < 0)
    throw new TypeError("Invalid action options");
  return { kind: "exact", version: v.expectedVersion as number };
}

export function decodeFxNodeAddNodeParams(value: unknown): AddNodeParams {
  try {
    const item = inspect(value);
    if (
      item &&
      item.keys.every((key) => ["typeId", "viewPosition", "nodeId"].includes(key)) &&
      ["typeId", "viewPosition"].every((key) => item.keys.includes(key))
    ) {
      const v = item.values,
        p = point(v.viewPosition);
      if (
        typeof v.typeId === "string" &&
        v.typeId &&
        v.typeId.length <= 128 &&
        p &&
        (!item.keys.includes("nodeId") ||
          v.nodeId === undefined ||
          (typeof v.nodeId === "string" && !!v.nodeId && v.nodeId.length <= 512))
      )
        return { typeId: v.typeId, viewPosition: p, ...(v.nodeId === undefined ? {} : { nodeId: v.nodeId as string }) };
    }
  } catch {}
  throw new TypeError("Invalid add-node parameters");
}
export function decodeFxNodeResourceAuthorization(value: unknown): FxNodeResourceAuthorization {
  let v: Values | undefined;
  try {
    v = data(value, ["viewId", "token", "graphVersion", "compositionRevision"]);
  } catch {}
  if (
    !v ||
    typeof v.viewId !== "string" ||
    !v.viewId ||
    v.viewId.length > 512 ||
    typeof v.token !== "string" ||
    !v.token ||
    v.token.length > 512 ||
    !Number.isSafeInteger(v.graphVersion) ||
    (v.graphVersion as number) < 0 ||
    !Number.isSafeInteger(v.compositionRevision) ||
    (v.compositionRevision as number) < 0
  )
    throw new TypeError("Invalid resource authorization");
  return Object.freeze({
    viewId: v.viewId,
    token: v.token,
    graphVersion: v.graphVersion as number,
    compositionRevision: v.compositionRevision as number,
  });
}
export function decodeFxNodeResourceData(value: unknown): FxNodeResourceData {
  let v: Values | undefined, length: number | undefined;
  try {
    v = data(value, ["name", "mime", "bytes"]);
    if (
      v &&
      typeof v.name === "string" &&
      v.name &&
      v.name.length <= 255 &&
      !/[\u0000-\u001f\u007f]/.test(v.name) &&
      typeof v.mime === "string" &&
      v.mime.length <= 128
    )
      length = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength")!.get!.call(v.bytes) as number;
  } catch {}
  if (!v || length === undefined) throw new TypeError("Invalid resource data");
  if (length === 0 || length > 32 * 1024 * 1024) throw new RangeError("Resource data is outside supported bounds");
  return Object.freeze({ name: v.name as string, mime: v.mime as string, bytes: v.bytes as ArrayBuffer });
}
