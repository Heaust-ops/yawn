import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export class Silhouette extends PostProcess {
  constructor(scene: Scene, options: { amount?: number; enabled?: boolean } = {}) { super(scene, "silhouette", options); }
}
