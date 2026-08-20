import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export class Edges extends PostProcess {
  constructor(scene: Scene, options: { amount?: number; enabled?: boolean } = {}) { super(scene, "edges", options); }
}
