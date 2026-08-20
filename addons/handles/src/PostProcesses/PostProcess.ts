import type { Scene } from "../Scene";

let nextPostProcess = 1;

/** Shared graph-membership behavior for the small post-process handles. */
export class PostProcess {
  readonly scene: Scene;
  readonly id: string;
  readonly kind: string;
  options: Record<string, unknown>;
  enabled: boolean;
  ready: Promise<void>;

  protected constructor(scene: Scene, kind: string, options: Record<string, unknown> = {}) {
    this.scene = scene;
    this.kind = kind;
    this.id = `${kind}-${nextPostProcess++}`;
    this.enabled = options.enabled !== false;
    const { enabled: _, ...effectOptions } = options;
    this.options = effectOptions;
    this.ready = scene.setPostProcess(this, this.enabled);
  }

  setEnabled(enabled: boolean) {
    this.enabled = enabled;
    this.ready = this.scene.setPostProcess(this, enabled);
    return this.ready;
  }

  update(options: Record<string, unknown>) {
    this.options = { ...this.options, ...options };
    this.ready = this.scene.setPostProcess(this, this.enabled);
    return this.ready;
  }

  dispose() { return this.setEnabled(false); }
}
