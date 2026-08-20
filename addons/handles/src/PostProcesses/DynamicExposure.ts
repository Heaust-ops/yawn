import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export class DynamicExposure extends PostProcess {
  constructor(scene: Scene, options: { exposure?: number; enabled?: boolean } = {}) { super(scene, "dynamicExposure", options); }
}
