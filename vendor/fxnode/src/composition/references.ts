import type { FxNodeComposition, FxNodeCompositionData, FxNodeMigrationStep, FxNodeUiRow } from "./types.js";

type K<T> = Extract<keyof T, string>;
type Visibility<P> =
  | { parameter: P; equals: string | number | boolean }
  | { parameter: P; in: readonly (string | number | boolean)[] }
  | { all: readonly Visibility<P>[] }
  | { any: readonly Visibility<P>[] };
type Vis<R, P> = R extends { visibleWhen: infer V } ? Omit<R, "visibleWhen"> & { visibleWhen: V & Visibility<P> } : R;
type Row<R, P, S, Resources> = R extends { kind: "parameter" | "widget" | "resource"; parameter: unknown }
  ? Vis<
      Omit<R, "parameter" | "resource"> & { parameter: P } & (R extends { kind: "resource" }
          ? { resource: Resources }
          : unknown),
      P
    >
  : R extends { kind: "socket"; socket: unknown }
    ? Vis<Omit<R, "socket"> & { socket: S }, P>
    : R extends { kind: "hidden"; target: "parameter" }
      ? Omit<R, "parameter"> & { parameter: P }
      : R extends { kind: "hidden"; target: "socket" }
        ? Omit<R, "socket"> & { socket: S }
        : R extends { widget: "grading-wheels"; bindings: infer B extends readonly unknown[] }
          ? Vis<Omit<R, "bindings"> & { bindings: { [I in keyof B]: B[I] & { scalar: P; color: P } } }, P>
          : Vis<R, P>;
/** @inline */
type Migration<M, P extends string, S extends string> = M extends { steps: infer Steps extends readonly unknown[] }
  ? M & {
      readonly fromVersion: number;
      readonly toVersion: number;
      readonly steps: readonly (Steps[number] & FxNodeMigrationStep<P, S>)[];
    }
  : never;
export type NodeReferenceCheck<
  C extends FxNodeCompositionData,
  D extends {
    parameters: object;
    sockets: object;
    ui: readonly unknown[];
    muteBypass: readonly unknown[];
    migrations: readonly { steps: readonly unknown[] }[];
  },
> = D &
  Omit<FxNodeComposition["nodes"][string], "style" | "parameters" | "sockets" | "ui" | "muteBypass" | "migrations"> & {
    readonly defaultSize?: never;
    readonly style: K<C["nodeStyles"]>;
    readonly parameters: {
      readonly [P in keyof D["parameters"]]: D["parameters"][P] &
        FxNodeComposition["nodes"][string]["parameters"][string];
    };
    readonly sockets: {
      readonly [S in keyof D["sockets"]]: D["sockets"][S] &
        FxNodeComposition["nodes"][string]["sockets"][string] & { readonly type: K<C["socketTypes"]> };
    };
    readonly ui: readonly (D["ui"][number] &
      FxNodeUiRow<K<D["parameters"]>, K<D["sockets"]>, K<C["resources"]>> &
      Row<D["ui"][number], K<D["parameters"]>, K<D["sockets"]>, K<C["resources"]>>)[];
    readonly muteBypass: readonly (readonly [K<D["sockets"]>, K<D["sockets"]>])[];
    readonly migrations: readonly Migration<D["migrations"][number], K<D["parameters"]>, K<D["sockets"]>>[];
  };
export type ReferenceCheck<C extends FxNodeCompositionData> = {
  readonly schemaVersion: 2;
  readonly id: string;
  readonly version: number;
  readonly compatibility: { readonly wildcardInputTypes: readonly K<C["socketTypes"]>[] };
  readonly theme: FxNodeComposition["theme"];
  readonly socketTypes: {
    readonly [T in keyof C["socketTypes"]]: C["socketTypes"][T] &
      FxNodeComposition["socketTypes"][string] & { readonly acceptsFrom: readonly K<C["socketTypes"]>[] };
  };
  readonly nodeStyles: {
    readonly [T in keyof C["nodeStyles"]]: C["nodeStyles"][T] & FxNodeComposition["nodeStyles"][string];
  };
  readonly resources: {
    readonly [T in keyof C["resources"]]: C["resources"][T] & FxNodeComposition["resources"][string];
  };
  readonly nodes: {
    readonly [N in keyof C["nodes"]]: C["nodes"][N] extends infer D extends {
      parameters: object;
      sockets: object;
      ui: readonly unknown[];
      muteBypass: readonly unknown[];
      migrations: readonly { steps: readonly unknown[] }[];
    }
      ? NodeReferenceCheck<C, D>
      : never;
  };
};
