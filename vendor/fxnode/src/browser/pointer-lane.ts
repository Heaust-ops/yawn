import type { InputEventWire } from "./protocol.js";

export type PointerMoveWire = Extract<InputEventWire, { kind: "pointer" }> & { phase: "move" };
export interface PointerLaneSnapshot {
  readonly sequence: number;
  readonly hostGeneration: number;
  readonly event: PointerMoveWire;
}

const WORDS = 16;
const STAMP = 0;
const FENCE = 1;
const SEQUENCE = 2;
const X_LOW = 3;
const X_HIGH = 4;
const Y_LOW = 5;
const Y_HIGH = 6;
const POINTER_ID = 7;
const POINTER_TYPE = 8;
const BUTTON = 9;
const BUTTONS = 10;
const MODIFIERS = 11;
const HOST_GENERATION_LOW = 12;
const HOST_GENERATION_HIGH = 13;
const UINT32 = 0x1_0000_0000;
const scratch = new DataView(new ArrayBuffer(8));
const views = new WeakMap<SharedArrayBuffer, Int32Array>();
const wordsFor = (buffer: SharedArrayBuffer): Int32Array => {
  const existing = views.get(buffer);
  if (existing) return existing;
  const words = new Int32Array(buffer);
  views.set(buffer, words);
  return words;
};

const pointerTypes = ["", "mouse", "pen", "touch"] as const;
const pointerTypeCode = (value: string): number => pointerTypes.indexOf(value as (typeof pointerTypes)[number]);
const writeFloat = (words: Int32Array, low: number, high: number, value: number): void => {
  scratch.setFloat64(0, value, true);
  Atomics.store(words, low, scratch.getInt32(0, true));
  Atomics.store(words, high, scratch.getInt32(4, true));
};
const readFloat = (words: Int32Array, low: number, high: number): number => {
  scratch.setInt32(0, Atomics.load(words, low), true);
  scratch.setInt32(4, Atomics.load(words, high), true);
  return scratch.getFloat64(0, true);
};

export function supportsPointerLane(): boolean {
  return (
    globalThis.crossOriginIsolated === true && typeof SharedArrayBuffer === "function" && typeof Atomics === "object"
  );
}

export function createPointerLane(): SharedArrayBuffer {
  return new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT * WORDS);
}
export function pointerLaneFence(buffer: SharedArrayBuffer): number {
  return Atomics.load(wordsFor(buffer), FENCE);
}
export function advancePointerLaneFence(buffer: SharedArrayBuffer): number {
  return (Atomics.add(wordsFor(buffer), FENCE, 1) + 1) | 0;
}

export function publishPointerMove(
  buffer: SharedArrayBuffer,
  event: PointerMoveWire,
  hostGeneration: number,
): number | undefined {
  const type = pointerTypeCode(event.pointerType);
  if (type < 1 || !Number.isSafeInteger(hostGeneration) || hostGeneration < 0) return undefined;
  const words = wordsFor(buffer);
  const writing = (Atomics.load(words, STAMP) + 1) | 1;
  Atomics.store(words, STAMP, writing);
  const sequence = (Atomics.load(words, SEQUENCE) + 1) | 0;
  Atomics.store(words, SEQUENCE, sequence);
  writeFloat(words, X_LOW, X_HIGH, event.position.x);
  writeFloat(words, Y_LOW, Y_HIGH, event.position.y);
  Atomics.store(words, POINTER_ID, event.pointerId);
  Atomics.store(words, POINTER_TYPE, type);
  Atomics.store(words, BUTTON, event.button);
  Atomics.store(words, BUTTONS, event.buttons);
  Atomics.store(words, MODIFIERS, event.modifiers);
  Atomics.store(words, HOST_GENERATION_LOW, hostGeneration % UINT32);
  Atomics.store(words, HOST_GENERATION_HIGH, Math.floor(hostGeneration / UINT32));
  Atomics.store(words, STAMP, (writing + 1) & ~1);
  return sequence;
}

export function readPointerMove(buffer: SharedArrayBuffer, consumedSequence?: number): PointerLaneSnapshot | undefined {
  const words = wordsFor(buffer);
  const before = Atomics.load(words, STAMP);
  if ((before & 1) !== 0) return undefined;
  const sequence = Atomics.load(words, SEQUENCE);
  if (sequence === 0 || sequence === consumedSequence) return undefined;
  const x = readFloat(words, X_LOW, X_HIGH);
  const y = readFloat(words, Y_LOW, Y_HIGH);
  const pointerId = Atomics.load(words, POINTER_ID);
  const pointerType = pointerTypes[Atomics.load(words, POINTER_TYPE)];
  const button = Atomics.load(words, BUTTON);
  const buttons = Atomics.load(words, BUTTONS);
  const modifiers = Atomics.load(words, MODIFIERS);
  const hostGeneration =
    (Atomics.load(words, HOST_GENERATION_LOW) >>> 0) + Atomics.load(words, HOST_GENERATION_HIGH) * UINT32;
  const after = Atomics.load(words, STAMP);
  if (before !== after || (after & 1) !== 0 || !pointerType || !Number.isFinite(x) || !Number.isFinite(y))
    return undefined;
  if (!Number.isSafeInteger(hostGeneration) || hostGeneration < 0) return undefined;
  return {
    sequence,
    hostGeneration,
    event: { kind: "pointer", phase: "move", pointerId, pointerType, position: { x, y }, button, buttons, modifiers },
  };
}
