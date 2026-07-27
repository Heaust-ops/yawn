import { deepFreeze } from "../core/json.js";
import { validateFxNodeComposition, type FxNodeCompositionIssue } from "./validate.js";
import { effectiveImageResourcePolicy } from "./resource-policy.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData, FxNodeReadonlyMap, FxNodeTheme } from "./types.js";
import type { ReferenceCheck } from "./references.js";

export class FxNodeCompositionError extends TypeError {
  constructor(readonly issues: readonly FxNodeCompositionIssue[]) {
    super(`Invalid fxnode composition (${issues.length} issue${issues.length === 1 ? "" : "s"})`);
    this.name = "FxNodeCompositionError";
  }
}
export const DEFAULT_FXNODE_THEME = {
  background: "#000000",
  grid: "#33363c",
  frame: "#30343a80",
  frameHeader: "#59616c",
  body: "#35383e",
  control: "#24272b",
  controlFill: "#4775b8",
  controlEditing: "#181a1d",
  textSelection: "#4775b8",
  outline: "#111216",
  text: "#e5e5e5",
  muted: "#a5a8ad",
  shadow: "#00000088",
  nodeSelected: "#ed5700",
  nodeActive: "#ffffff",
  unknownHeader: "#555b64",
  unknownSocket: "#999999",
  linkMuted: "#d94b4b",
  knifeMuted: "#e85b5b",
  emphasis: "#ffffff",
  focus: "#f5a623",
  editOutline: "#666a70",
  resize: "#8b8e95",
  muteOverlay: "#14141459",
  boxSelectionFill: "#f5a6231f",
  checkerLight: "#aaaaaa",
  checkerDark: "#777777",
  widgetBorder: "#111216",
  rampBorder: "#111111",
  resourceBackground: "#202228",
} as const satisfies FxNodeTheme;
function facade<K, V>(entries: readonly (readonly [K, V])[]): FxNodeReadonlyMap<K, V> {
  const map = new Map<K, V>(entries);
  return Object.freeze({
    get size() {
      return map.size;
    },
    get: (key: K) => map.get(key),
    has: (key: K) => map.has(key),
    keys: () => map.keys(),
    values: () => map.values(),
    entries: () => map.entries(),
    forEach: (callback: (value: V, key: K) => void) => map.forEach((value, key) => callback(value, key)),
    [Symbol.iterator]: () => map[Symbol.iterator](),
  });
}
export function compileFxNodeComposition<const C extends FxNodeCompositionData>(
  composition: C & ReferenceCheck<C>,
): CompiledFxNodeComposition<C> {
  const checked = validateFxNodeComposition(composition);
  if (!checked.ok) throw new FxNodeCompositionError(checked.issues);
  let source: C;
  try {
    source = deepFreeze(structuredClone(checked.value)) as unknown as C;
  } catch {
    throw new FxNodeCompositionError(
      Object.freeze([Object.freeze({ code: "data.clone", path: "", message: "composition could not be cloned" })]),
    );
  }
  const raw = source as C & { id: string; version: number; theme: unknown };
  const withIds = (record: Record<string, unknown>) =>
    Object.entries(record).map(([id, value]) => [id, deepFreeze({ ...(value as object), id })] as const);
  const nodeEntries = Object.entries(raw.nodes as Record<string, unknown>).map(
    ([typeId, value]) => [typeId, deepFreeze({ ...(value as object), typeId })] as const,
  );
  const resourceEntries = Object.entries(
    raw.resources as Record<string, import("./types.js").FxNodeImageResourceDefinition>,
  ).map(([id, value]) => [id, deepFreeze({ ...effectiveImageResourcePolicy(value), id })] as const);
  return Object.freeze({
    source,
    id: raw.id,
    version: raw.version,
    compatibility: source.compatibility,
    theme: raw.theme,
    nodes: facade(nodeEntries),
    socketTypes: facade(withIds(raw.socketTypes as Record<string, unknown>)),
    styles: facade(withIds(raw.nodeStyles as Record<string, unknown>)),
    resources: facade(resourceEntries),
  }) as unknown as CompiledFxNodeComposition<C>;
}
export function createInitialFxNodeComposition(
  applicationId: string,
  applicationVersion: number,
  resources: FxNodeCompositionData["resources"],
): CompiledFxNodeComposition<FxNodeCompositionData> {
  return compileFxNodeComposition({
    schemaVersion: 2,
    id: applicationId,
    version: applicationVersion,
    resources,
    nodeStyles: {},
    compatibility: { wildcardInputTypes: [] as readonly never[] },
    socketTypes: {},
    nodes: {},
    theme: DEFAULT_FXNODE_THEME,
  }) as unknown as CompiledFxNodeComposition<FxNodeCompositionData>;
}
