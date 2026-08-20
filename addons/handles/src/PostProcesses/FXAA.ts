import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export class FXAA extends PostProcess {
  constructor(scene: Scene, options: { enabled?: boolean } = {}) { super(scene, "fxaa", options); }
}
