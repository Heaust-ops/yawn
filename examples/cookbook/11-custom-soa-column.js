/** Allocate one SIMD-aligned velocity row for every live instance slot. */
export function createVelocityColumn(core) {
  return core.allocateArray({
    name: "instance.velocity",
    domain: "instance",
    scalar: "f32",
    lanes: 4,
  });
}

/** Mutate an existing row directly; no renderer command message is emitted. */
export function setVelocity(column, instance, xyz) {
  column.write(instance.handle[0], [xyz[0], xyz[1], xyz[2], 0]);
}
