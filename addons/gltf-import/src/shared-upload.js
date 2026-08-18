const MAGIC = 0x414f5359;

/** Publishes one byte payload into a packed fixed SOA allocation. */
export function writeSharedUpload(buffer, descriptor, bytes) {
  if (!(buffer instanceof SharedArrayBuffer) || !(bytes instanceof Uint8Array))
    throw new TypeError("shared upload requires SharedArrayBuffer storage and Uint8Array bytes");
  if (
    !descriptor ||
    descriptor.domain !== "fixed" ||
    descriptor.scalar !== "u32" ||
    descriptor.stride !== descriptor.lanes * 4 ||
    descriptor.controlPtr % 64 !== 0 ||
    descriptor.dataOffset !== 64 ||
    bytes.byteLength < 1 ||
    bytes.byteLength > descriptor.length * descriptor.lanes * 4
  ) throw new TypeError("invalid packed fixed SOA upload");

  const control = new Int32Array(buffer, descriptor.controlPtr, 16);
  if (
    (Atomics.load(control, 0) >>> 0) !== MAGIC ||
    (Atomics.load(control, 2) >>> 0) !== descriptor.id
  ) throw new Error("SOA_PROTOCOL_MISMATCH");

  let sequence;
  for (let attempt = 0; attempt < 1024; attempt++) {
    const candidate = Atomics.load(control, 9) >>> 0;
    if (!(candidate & 1) && (Atomics.compareExchange(control, 9, candidate | 0, (candidate + 1) | 0) >>> 0) === candidate) {
      sequence = candidate;
      break;
    }
  }
  if (sequence === undefined) throw new Error("SOA_BUSY");
  try {
    new Uint8Array(
      buffer,
      descriptor.controlPtr + descriptor.dataOffset,
      bytes.byteLength,
    ).set(bytes);
  } finally {
    Atomics.store(control, 9, (sequence + 2) | 0);
    Atomics.notify(control, 9);
  }
}
