const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

class Handle {
  constructor(array, row = 0) {
    this.array = array;
    this.row = row;
  }
  get state() { return this.array.read(this.row); }
  set state(value) { this.array.write(this.row, value); }
  patch(offset, values) {
    const state = this.state;
    state.splice(offset, values.length, ...values);
    this.state = state;
  }
}

/** Conventional camera values over one caller-owned shared row. */
export class CameraHandle extends Handle {
  static async create(core, name = "camera") {
    const array = await core.allocateRows({ name, rows: 1, stride: 64, format: "f32" });
    const camera = new CameraHandle(array);
    camera.state = [0, 0, 4, 0, 0, 0, 0, 0, 0, 1, 0, 0, Math.PI / 3, 1, 0.1, 1000];
    return camera;
  }
  get position() { return this.state.slice(0, 3); }
  set position(value) { this.patch(0, value); }
  get target() { return this.state.slice(4, 7); }
  set target(value) { this.patch(4, value); }
}

/** Conventional material properties over one eight-float shared row. */
export class MaterialHandle extends Handle {
  static async create(core, name = "material") {
    const material = new MaterialHandle(await core.allocateRows({ name, rows: 1, stride: 32, format: "f32" }));
    material.state = [1, 1, 1, 1, 0, 1, 0, 0];
    return material;
  }
  get baseColor() { return this.state.slice(0, 4); }
  set baseColor(value) { this.patch(0, value); }
  get metallic() { return this.state[4]; }
  set metallic(value) { this.patch(4, [value]); }
  get roughness() { return this.state[5]; }
  set roughness(value) { this.patch(5, [value]); }
}

/** Conventional mesh transform over one SIMD-aligned shared row. */
export class MeshHandle extends Handle {
  static async create(core, name = "mesh") {
    const mesh = new MeshHandle(await core.allocateRows({ name, rows: 1, stride: 64, format: "f32" }));
    mesh.transform = identity;
    return mesh;
  }
  get transform() { return this.state; }
  set transform(value) { this.state = value; }
}
