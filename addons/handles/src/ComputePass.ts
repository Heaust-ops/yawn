import type { Scene } from "./Scene";

export type GraphBuffer = {
  id: string;
  array: string;
  usage?: string[];
};

export type GraphTexture = {
  id: string;
  format?: string;
  size?: [number | "canvas", number | "canvas", number?];
  usage?: string[];
  transient?: boolean;
};

export type GraphSampler = {
  id: string;
  magFilter?: "nearest" | "linear";
  minFilter?: "nearest" | "linear";
};

export type GraphBinding = {
  group: number;
  binding: number;
  resource: string;
};

export type ComputePassOptions = {
  id?: string;
  code: string;
  entry?: string;
  dispatch?: [number, number?, number?];
  after?: string[];
  bindings?: GraphBinding[];
  buffers?: GraphBuffer[];
  textures?: GraphTexture[];
  samplers?: GraphSampler[];
};

let nextComputePass = 1;

/** A graph-authored compute pass; attaching or updating it rebuilds the Scene loadout. */
export class ComputePass {
  readonly id: string;
  code: string;
  entry: string;
  dispatch: [number, number, number];
  after: string[];
  bindings: GraphBinding[];
  buffers: GraphBuffer[];
  textures: GraphTexture[];
  samplers: GraphSampler[];
  #scene?: Scene;

  constructor(options: ComputePassOptions) {
    if (!options?.code) throw new TypeError("ComputePass code is required");
    this.id = options.id ?? `compute-${nextComputePass++}`;
    this.code = options.code;
    this.entry = options.entry ?? "main";
    this.dispatch = [
      options.dispatch?.[0] ?? 1,
      options.dispatch?.[1] ?? 1,
      options.dispatch?.[2] ?? 1,
    ];
    this.after = [...(options.after ?? [])];
    this.bindings = [...(options.bindings ?? [])];
    this.buffers = [...(options.buffers ?? [])];
    this.textures = [...(options.textures ?? [])];
    this.samplers = [...(options.samplers ?? [])];
  }

  update(options: Partial<Omit<ComputePassOptions, "id">>) {
    if (options.code !== undefined) this.code = options.code;
    if (options.entry !== undefined) this.entry = options.entry;
    if (options.dispatch !== undefined) this.dispatch = [
      options.dispatch[0],
      options.dispatch[1] ?? 1,
      options.dispatch[2] ?? 1,
    ];
    if (options.after !== undefined) this.after = [...options.after];
    if (options.bindings !== undefined) this.bindings = [...options.bindings];
    if (options.buffers !== undefined) this.buffers = [...options.buffers];
    if (options.textures !== undefined) this.textures = [...options.textures];
    if (options.samplers !== undefined) this.samplers = [...options.samplers];
    return this.#scene?.updateRenderGraph() ?? Promise.resolve();
  }

  attach(scene?: Scene) {
    this.#scene = scene;
  }
}
