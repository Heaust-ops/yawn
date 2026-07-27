export interface FxNodeCompositionData {
  readonly schemaVersion: 2;
  readonly id: string;
  readonly version: number;
  readonly compatibility: { readonly wildcardInputTypes: readonly string[] };
  readonly socketTypes: Readonly<Record<string, FxNodeSocketTypeDefinition>>;
  readonly theme: FxNodeTheme;
  readonly nodeStyles: Readonly<Record<string, FxNodeStyleDefinition>>;
  readonly resources: Readonly<Record<string, FxNodeResourceDefinition>>;
  readonly nodes: Readonly<Record<string, FxNodeDefinition>>;
}
/** An authoring seed which may omit the theme until passed through setTheme. */
export type FxNodeCompositionSeed = Omit<FxNodeCompositionData, "theme"> & { readonly theme?: never };

export type NodeTypeId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  string extends Extract<keyof C["nodes"], string> ? string : Extract<keyof C["nodes"], string>;
export type SocketTypeId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  string extends Extract<keyof C["socketTypes"], string> ? string : Extract<keyof C["socketTypes"], string>;
export type NodeStyleId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  string extends Extract<keyof C["nodeStyles"], string> ? string : Extract<keyof C["nodeStyles"], string>;
export type ResourceId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  string extends Extract<keyof C["resources"], string> ? string : Extract<keyof C["resources"], string>;
export type NodeParameterId<C extends FxNodeCompositionData, N extends NodeTypeId<C>> = N extends keyof C["nodes"]
  ? C["nodes"][N] extends { readonly parameters: infer P }
    ? Extract<keyof P, string>
    : string
  : string;
export type NodeSocketId<C extends FxNodeCompositionData, N extends NodeTypeId<C>> = N extends keyof C["nodes"]
  ? C["nodes"][N] extends { readonly sockets: infer S }
    ? Extract<keyof S, string>
    : string
  : string;
export type AnyNodeParameterId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  NodeTypeId<C> extends infer N ? (N extends NodeTypeId<C> ? NodeParameterId<C, N> : never) : never;
export type AnyNodeSocketId<C extends FxNodeCompositionData = FxNodeCompositionData> =
  NodeTypeId<C> extends infer N ? (N extends NodeTypeId<C> ? NodeSocketId<C, N> : never) : never;

export type FxNodeHexColor = `#${string}`;
export type FxNodeJsonValue =
  | null
  | boolean
  | number
  | string
  | readonly FxNodeJsonValue[]
  | { readonly [key: string]: FxNodeJsonValue };
export type FxNodeParameterValue =
  | { readonly kind: "number"; readonly value: number }
  | { readonly kind: "string"; readonly value: string }
  | { readonly kind: "boolean"; readonly value: boolean }
  | { readonly kind: "vector"; readonly value: readonly [number, number, number] }
  | { readonly kind: "color"; readonly value: readonly [number, number, number, number] }
  | { readonly kind: "json"; readonly value: FxNodeJsonValue };
/** @inline */
/** @inline */
type Bounds = {
  readonly minimum?: number;
  readonly maximum?: number;
  readonly softMin?: number;
  readonly softMax?: number;
  readonly step?: number;
};
export type FxNodeValueSchema =
  | (Bounds & {
      readonly type: "number";
      readonly default: Extract<FxNodeParameterValue, { kind: "number" }>;
      readonly integer?: boolean;
      readonly precision?: number;
    })
  | {
      readonly type: "string";
      readonly default: Extract<FxNodeParameterValue, { kind: "string" }>;
      readonly enum?: readonly string[];
    }
  | { readonly type: "boolean"; readonly default: Extract<FxNodeParameterValue, { kind: "boolean" }> }
  | (Bounds & { readonly type: "vector"; readonly default: Extract<FxNodeParameterValue, { kind: "vector" }> })
  | (Bounds & { readonly type: "color"; readonly default: Extract<FxNodeParameterValue, { kind: "color" }> })
  | {
      readonly type: "json";
      readonly codec?: "color-ramp/v1";
      readonly default: Extract<FxNodeParameterValue, { kind: "json" }>;
    };

export type FxNodeVisibility<P extends string = string> =
  | { readonly parameter: P; readonly equals: string | number | boolean }
  | { readonly parameter: P; readonly in: readonly (string | number | boolean)[] }
  | { readonly all: readonly FxNodeVisibility<P>[] }
  | { readonly any: readonly FxNodeVisibility<P>[] };
export interface FxNodeSocketTypeDefinition<S extends string = string> {
  readonly title: string;
  readonly color: FxNodeHexColor;
  readonly acceptsFrom: readonly S[];
}
export interface FxNodeStyleDefinition {
  readonly header: FxNodeHexColor;
}
export interface FxNodeImageResourceDefinition {
  readonly kind: "image";
  readonly title: string;
  readonly openTitle: string;
  readonly accept: readonly string[];
  readonly referencePrefix: string;
  readonly maxBytes: number;
  readonly maxWidth: number;
  readonly maxHeight: number;
  readonly maxPixels: number;
}
export type FxNodeResourceDefinition = FxNodeImageResourceDefinition;
export interface FxNodeSocketDefinition<S extends string = string> {
  readonly title: string;
  readonly direction: "input" | "output";
  readonly type: S;
  readonly maxIncomingLinks: number;
  readonly visible: boolean;
  readonly value: FxNodeValueSchema | null;
  readonly showValue: boolean;
}
export interface FxNodeGradingBinding<P extends string = string> {
  readonly title: string;
  readonly scalar: P;
  readonly color: P;
}
export type FxNodeUiRow<P extends string = string, S extends string = string, R extends string = string> =
  | {
      readonly kind: "parameter";
      readonly parameter: P;
      readonly title?: string;
      readonly visibleWhen?: FxNodeVisibility<P>;
    }
  | { readonly kind: "socket"; readonly socket: S; readonly title?: string; readonly visibleWhen?: FxNodeVisibility<P> }
  | {
      readonly kind: "widget";
      readonly widget: "color-ramp";
      readonly parameter: P;
      readonly title?: string;
      readonly visibleWhen?: FxNodeVisibility<P>;
    }
  | {
      readonly kind: "widget";
      readonly widget: "grading-wheels";
      readonly bindings: readonly [FxNodeGradingBinding<P>, FxNodeGradingBinding<P>, FxNodeGradingBinding<P>];
      readonly visibleWhen?: FxNodeVisibility<P>;
    }
  | {
      readonly kind: "resource";
      readonly resource: R;
      readonly parameter: P;
      readonly title?: string;
      readonly openTitle?: string;
      readonly visibleWhen?: FxNodeVisibility<P>;
    }
  | {
      readonly kind: "text";
      readonly variant: "header" | "category" | "section" | "panel" | "placeholder";
      readonly title: string;
      readonly visibleWhen?: FxNodeVisibility<P>;
    }
  | { readonly kind: "hidden"; readonly target: "parameter"; readonly parameter: P }
  | { readonly kind: "hidden"; readonly target: "socket"; readonly socket: S };
export type FxNodeMigrationStep<P extends string = string, S extends string = string> =
  | { readonly kind: "materialize-missing"; readonly target: "parameter"; readonly key: P }
  | { readonly kind: "materialize-missing"; readonly target: "socket"; readonly key: S }
  | { readonly kind: "migrate-parameter"; readonly parameter: P; readonly codec: "color-ramp/legacy-stops" }
  | { readonly kind: "rename-parameter"; readonly from: string; readonly to: P }
  | { readonly kind: "rename-socket"; readonly from: string; readonly to: S };
export interface FxNodeMigration<P extends string = string, S extends string = string> {
  readonly fromVersion: number;
  readonly toVersion: number;
  readonly steps: readonly FxNodeMigrationStep<P, S>[];
}
export interface FxNodeDefinition {
  readonly version: number;
  readonly title: string;
  readonly behavior: "standard" | "frame" | "reroute";
  readonly style: string;
  readonly parameters: Readonly<Record<string, FxNodeValueSchema>>;
  readonly sockets: Readonly<Record<string, FxNodeSocketDefinition>>;
  readonly ui: readonly FxNodeUiRow[];
  readonly muteBypass: readonly (readonly [string, string])[];
  readonly migrations: readonly FxNodeMigration[];
}
export interface FxNodeTheme {
  readonly background: FxNodeHexColor;
  readonly grid: FxNodeHexColor;
  readonly frame: FxNodeHexColor;
  readonly frameHeader: FxNodeHexColor;
  readonly body: FxNodeHexColor;
  readonly control: FxNodeHexColor;
  readonly controlFill: FxNodeHexColor;
  readonly controlEditing: FxNodeHexColor;
  readonly textSelection: FxNodeHexColor;
  readonly outline: FxNodeHexColor;
  readonly text: FxNodeHexColor;
  readonly muted: FxNodeHexColor;
  readonly shadow: FxNodeHexColor;
  readonly nodeSelected: FxNodeHexColor;
  readonly nodeActive: FxNodeHexColor;
  readonly unknownHeader: FxNodeHexColor;
  readonly unknownSocket: FxNodeHexColor;
  readonly linkMuted: FxNodeHexColor;
  readonly knifeMuted: FxNodeHexColor;
  readonly emphasis: FxNodeHexColor;
  readonly focus: FxNodeHexColor;
  readonly editOutline: FxNodeHexColor;
  readonly resize: FxNodeHexColor;
  readonly muteOverlay: FxNodeHexColor;
  readonly boxSelectionFill: FxNodeHexColor;
  readonly checkerLight: FxNodeHexColor;
  readonly checkerDark: FxNodeHexColor;
  readonly widgetBorder: FxNodeHexColor;
  readonly rampBorder: FxNodeHexColor;
  readonly resourceBackground: FxNodeHexColor;
}
export interface FxNodeComposition extends FxNodeCompositionData {
  readonly schemaVersion: 2;
  readonly compatibility: { readonly wildcardInputTypes: readonly string[] };
  readonly theme: FxNodeTheme;
  readonly socketTypes: Readonly<Record<string, FxNodeSocketTypeDefinition>>;
  readonly nodeStyles: Readonly<Record<string, FxNodeStyleDefinition>>;
  readonly resources: Readonly<Record<string, FxNodeResourceDefinition>>;
  readonly nodes: Readonly<Record<string, FxNodeDefinition>>;
}
export interface FxNodeReadonlyMap<K, V> extends Iterable<readonly [K, V]> {
  readonly size: number;
  get(key: K): V | undefined;
  has(key: K): boolean;
  keys(): IterableIterator<K>;
  values(): IterableIterator<V>;
  entries(): IterableIterator<[K, V]>;
  forEach(callback: (value: V, key: K) => void): void;
  [Symbol.iterator](): IterableIterator<[K, V]>;
}
/** @inline */
/** @inline */
type Values<T> = T[keyof T];
export type CompiledNode<C extends FxNodeCompositionData, N extends NodeTypeId<C>> = (N extends keyof C["nodes"]
  ? C["nodes"][N]
  : never) & { readonly typeId: N };
export type CompiledResource<C extends FxNodeCompositionData, R extends ResourceId<C>> = Omit<
  C["resources"][R],
  "maxBytes" | "maxWidth" | "maxHeight" | "maxPixels"
> &
  Pick<FxNodeImageResourceDefinition, "maxBytes" | "maxWidth" | "maxHeight" | "maxPixels"> & { readonly id: R };
export interface CompiledFxNodeComposition<C extends FxNodeCompositionData = FxNodeComposition> {
  readonly source: C;
  readonly id: C["id"];
  readonly version: C["version"];
  readonly compatibility: C["compatibility"];
  readonly theme: C["theme"];
  readonly nodes: FxNodeReadonlyMap<NodeTypeId<C>, Values<{ [N in NodeTypeId<C>]: CompiledNode<C, N> }>>;
  readonly socketTypes: FxNodeReadonlyMap<
    SocketTypeId<C>,
    Values<{ [S in SocketTypeId<C>]: C["socketTypes"][S] & { readonly id: S } }>
  >;
  readonly styles: FxNodeReadonlyMap<
    NodeStyleId<C>,
    Values<{ [S in NodeStyleId<C>]: C["nodeStyles"][S] & { readonly id: S } }>
  >;
  readonly resources: FxNodeReadonlyMap<ResourceId<C>, Values<{ [R in ResourceId<C>]: CompiledResource<C, R> }>>;
}
