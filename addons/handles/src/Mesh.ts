import { Node, type NodeOptions } from "./Node";
import type { Scene } from "./Scene";
import type { PBRMaterial } from "./materials/PBRMaterial";

export const VertexKinds = Object.freeze(["positions", "normals", "tangents", "uvs", "colors", "indices"] as const);
export type VertexKind = (typeof VertexKinds)[number];
export type MeshOptions = NodeOptions & {
  geometryId?: number;
  material?: PBRMaterial;
  vertexData?: Partial<Record<VertexKind, ArrayLike<number>>>;
  visible?: boolean;
};

/** A renderable Node; clones share geometry until either clone mutates vertex data. */
export class Mesh extends Node {
  geometryId: number;
  vertexCount = 0;
  indexCount = 0;
  instanceOf = -1;
  readonly faceMaterials = new Map<number, number>();
  #registered = false;

  constructor(scene: Scene, options: MeshOptions = {}) {
    super(scene, options);
    this.geometryId = options.geometryId ?? scene.createGeometry();
    const nodeReady = this.ready;
    this.ready = nodeReady.then(async () => {
      if (options.material) await options.material.ready;
      await scene.ensureRows(`mesh.${this.id}.faceMaterials`, 1, 16, "u32");
      this.vertexCount = Number(scene.geometryData(this.geometryId, "positions")?.length ?? 0) / 3;
      this.indexCount = Number(scene.geometryData(this.geometryId, "indices")?.length ?? 0);
      this.instanceOf = options.geometryId === undefined ? this.id : this.geometryId;
      scene.array("meshInfo").write(this.id, [
        this.geometryId,
        options.material?.id ?? 0,
        options.visible === false ? 0 : 1,
        this.instanceOf,
      ]);
      for (const [kind, data] of Object.entries(options.vertexData ?? {}))
        await this.#setVertexData(kind as VertexKind, data!, false);
      this.#writeBounds();
      this.#registered = true;
      await scene.registerMesh(this);
      return this;
    });
  }

  get isVisible() {
    if (this.id < 0) throw new Error("Await mesh.ready before reading it");
    return this.scene.array("meshInfo").row(this.id)[2] !== 0;
  }

  set isVisible(value: boolean) {
    if (this.id < 0) throw new Error("Await mesh.ready before writing it");
    this.scene.array("meshInfo").row(this.id)[2] = value ? 1 : 0;
  }

  get materialId() {
    return this.scene.array("meshInfo").row(this.id)[1];
  }

  set material(value: PBRMaterial) {
    if (value.scene !== this.scene || value.id < 0) throw new Error("Await a material from the same Scene");
    this.scene.array("meshInfo").row(this.id)[1] = value.id;
  }

  clone(options: Omit<MeshOptions, "geometryId" | "vertexData"> = {}) {
    if (this.id < 0) throw new Error("Await mesh.ready before cloning it");
    return new Mesh(this.scene, { ...options, geometryId: this.geometryId });
  }

  async setVertexData(kind: VertexKind, data: ArrayLike<number>) {
    if (!VertexKinds.includes(kind)) throw new TypeError(`Unknown vertex kind: ${kind}`);
    await this.ready;
    await this.#setVertexData(kind, data, true);
    return this;
  }

  async #setVertexData(kind: VertexKind, data: ArrayLike<number>, makeUnique: boolean) {
    if (makeUnique && this.scene.geometryReferences(this.geometryId) > 1) {
      const original = this.geometryId;
      this.geometryId = await this.scene.cloneGeometry(original);
      this.scene.releaseGeometry(original);
      this.scene.referenceGeometry(this.geometryId);
      this.instanceOf = this.id;
      this.scene.array("meshInfo").row(this.id).set([this.geometryId, this.materialId, this.isVisible ? 1 : 0, this.id]);
    }
    if (kind === "positions") this.vertexCount = data.length / 3;
    if (kind === "indices") this.indexCount = data.length;
    await this.scene.setVertexData(this.geometryId, kind, data, makeUnique);
    if (kind === "positions") this.#writeBounds();
  }

  async setMaterialForFaces(material: PBRMaterial, faces: number | number[]) {
    await Promise.all([this.ready, material.ready]);
    if (material.scene !== this.scene) throw new Error("Material must belong to the same Scene");
    const list = Array.isArray(faces) ? faces : [faces];
    const maximum = Math.max(...list);
    const array = await this.scene.ensureRows(`mesh.${this.id}.faceMaterials`, maximum + 1, 16, "u32");
    for (const face of list) {
      if (!Number.isInteger(face) || face < 0) throw new RangeError("face");
      array.row(face)[0] = material.id + 1;
      this.faceMaterials.set(face, material.id);
    }
    await this.scene.updateRenderGraph();
    return this;
  }

  #writeBounds() {
    const positions = this.scene.geometryData(this.geometryId, "positions");
    if (!positions?.length || this.id < 0) return;
    const minimum = [Infinity, Infinity, Infinity];
    const maximum = [-Infinity, -Infinity, -Infinity];
    for (let index = 0; index < positions.length; index += 3)
      for (let lane = 0; lane < 3; lane++) {
        minimum[lane] = Math.min(minimum[lane], positions[index + lane]);
        maximum[lane] = Math.max(maximum[lane], positions[index + lane]);
      }
    this.scene.array("bounds").row(this.id).set([...minimum, 0, ...maximum, 0]);
  }

  override async dispose() {
    await this.ready;
    if (this.disposed) return;
    if (this.#registered) {
      this.#registered = false;
      await this.scene.unregisterMesh(this);
    }
    await this.scene.core.deleteRows(`mesh.${this.id}.faceMaterials`);
    await super.dispose();
  }
}
