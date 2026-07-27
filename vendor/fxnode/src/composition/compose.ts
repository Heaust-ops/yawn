import type { NodeReferenceCheck } from "./references.js";
import type {
  FxNodeCompositionData,
  FxNodeDefinition,
  FxNodeSocketTypeDefinition,
  FxNodeStyleDefinition,
  FxNodeTheme,
} from "./types.js";

/** @inline */
type ReplaceProperty<T, K extends keyof T, V> = { readonly [P in keyof T]: P extends K ? V : T[P] };
/** @inline */
type PutProperty<T, K extends PropertyKey, V> = {
  readonly [P in keyof T | K]: P extends K ? V : P extends keyof T ? T[P] : never;
};
type EntryValue<E, K extends PropertyKey> = E extends readonly [K, infer V] ? V : never;
type PutEntries<T, E extends readonly [string, unknown]> = {
  readonly [P in keyof T | E[0]]: P extends E[0] ? EntryValue<E, P> : P extends keyof T ? T[P] : never;
};
export type NodeCompositionEntry = readonly [id: string, definition: FxNodeDefinition];
export type SocketCompositionEntry = readonly [id: string, definition: FxNodeSocketTypeDefinition];
export type ComposedNode<C extends FxNodeCompositionData, I extends string, D extends FxNodeDefinition> = {
  readonly [P in keyof C]: P extends "nodes"
    ? { readonly [N in keyof C["nodes"] | I]: N extends I ? D : N extends keyof C["nodes"] ? C["nodes"][N] : never }
    : C[P];
};
export type ComposedNodes<C extends FxNodeCompositionData, E extends NodeCompositionEntry> = ReplaceProperty<
  C,
  "nodes",
  PutEntries<C["nodes"], E>
>;
export type ComposedSocket<
  C extends Pick<FxNodeCompositionData, "socketTypes">,
  I extends string,
  D extends FxNodeSocketTypeDefinition,
> = {
  readonly [P in keyof C]: P extends "socketTypes"
    ? {
        readonly [S in keyof C["socketTypes"] | I]: S extends I
          ? D
          : S extends keyof C["socketTypes"]
            ? C["socketTypes"][S]
            : never;
      }
    : C[P];
};
export type ComposedSockets<
  C extends Pick<FxNodeCompositionData, "socketTypes">,
  E extends SocketCompositionEntry,
> = ReplaceProperty<C, "socketTypes", PutEntries<C["socketTypes"], E>>;
/** @inline */
type ThemeTarget = Omit<FxNodeCompositionData, "theme"> & { readonly theme?: FxNodeTheme };
export type Themed<C, T> = {
  readonly [P in keyof C | "theme"]: P extends "theme" ? T : P extends keyof C ? C[P] : never;
};
export type RemovedSocket<
  C extends Pick<FxNodeCompositionData, "socketTypes">,
  I extends Extract<keyof C["socketTypes"], string>,
> = { readonly [P in keyof C]: P extends "socketTypes" ? Omit<C["socketTypes"], I> : C[P] };
export type RemovedNode<C extends Pick<FxNodeCompositionData, "nodes">, I extends Extract<keyof C["nodes"], string>> = {
  readonly [P in keyof C]: P extends "nodes" ? Omit<C["nodes"], I> : C[P];
};

export function setTheme<const C extends ThemeTarget, const T extends FxNodeTheme>(
  composition: C,
  theme: T,
): Themed<C, T>;
export function setTheme(composition: ThemeTarget, theme: FxNodeTheme): FxNodeCompositionData {
  return { ...composition, theme };
}
export type HeaderStyled<C, S> = { readonly [P in keyof C]: P extends "nodeStyles" ? S : C[P] };
export function setHeaderStyles<
  const C extends Pick<FxNodeCompositionData, "nodeStyles">,
  const S extends Readonly<Record<string, FxNodeStyleDefinition>>,
>(composition: C, styles: S): HeaderStyled<C, S>;
export function setHeaderStyles(
  composition: Pick<FxNodeCompositionData, "nodeStyles">,
  styles: Readonly<Record<string, FxNodeStyleDefinition>>,
): Pick<FxNodeCompositionData, "nodeStyles"> {
  return { ...composition, nodeStyles: styles };
}
export function composeSocket<
  const C extends Pick<FxNodeCompositionData, "socketTypes">,
  const I extends string,
  const D extends FxNodeSocketTypeDefinition<Extract<keyof NoInfer<C>["socketTypes"], string> | I>,
>(composition: C, id: I, definition: D): ComposedSocket<C, I, D>;
export function composeSocket(
  composition: Pick<FxNodeCompositionData, "socketTypes">,
  id: string,
  definition: FxNodeSocketTypeDefinition,
): Pick<FxNodeCompositionData, "socketTypes"> {
  return { ...composition, socketTypes: { ...composition.socketTypes, [id]: definition } };
}
export function removeSocket<
  const C extends Pick<FxNodeCompositionData, "socketTypes">,
  const I extends Extract<keyof C["socketTypes"], string>,
>(composition: C, id: I): RemovedSocket<C, I>;
export function removeSocket(
  composition: Pick<FxNodeCompositionData, "socketTypes">,
  id: string,
): Pick<FxNodeCompositionData, "socketTypes"> {
  const { [id]: _, ...socketTypes } = composition.socketTypes;
  return { ...composition, socketTypes };
}
export function composeNode<
  const D extends FxNodeDefinition,
  const C extends FxNodeCompositionData,
  const I extends string,
>(composition: C, id: I, definition: D & NodeReferenceCheck<NoInfer<C>, D>): ComposedNode<C, I, D>;
export function composeNode(
  composition: FxNodeCompositionData,
  id: string,
  definition: FxNodeDefinition,
): FxNodeCompositionData {
  return { ...composition, nodes: { ...composition.nodes, [id]: definition } };
}
export function removeNode<
  const C extends Pick<FxNodeCompositionData, "nodes">,
  const I extends Extract<keyof C["nodes"], string>,
>(composition: C, id: I): RemovedNode<C, I>;
export function removeNode(
  composition: Pick<FxNodeCompositionData, "nodes">,
  id: string,
): Pick<FxNodeCompositionData, "nodes"> {
  const { [id]: _, ...nodes } = composition.nodes;
  return { ...composition, nodes };
}
