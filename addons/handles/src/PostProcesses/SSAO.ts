import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export class SSAO extends PostProcess {
  constructor(scene: Scene, options: { amount?: number; enabled?: boolean } = {}) { super(scene, "ssao", options); }
}
