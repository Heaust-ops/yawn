import { PostProcess } from "./PostProcess";
import type { Scene } from "../Scene";

export type ToneMap = "aces" | "reinhard" | "linear";
export class ColorGrading extends PostProcess {
  constructor(scene: Scene, options: { amount?: number; toneMap?: ToneMap; enabled?: boolean } = {}) {
    super(scene, "colorGrading", options);
  }
}
