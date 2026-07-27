import type { FxNodeCompositionData, NodeTypeId } from "../composition/types.js";
import {
  PERSISTENCE_LIMITS,
  type DecodeResult as BoundDecodeResult,
  type ValidationIssue as BoundValidationIssue,
} from "../composition/bound-document.js";
import { saveCompositionCompatibility } from "../composition/save-compatibility.js";
import { FXNODE_COMPOSITION_LIMITS, validateFxNodeComposition } from "../composition/validate.js";
import { admitStructuredData, deepFreeze, isRecord } from "../core/json.js";
import type { GraphDocument, GraphLayoutV2 } from "../core/types.js";
import type { CompatibleFxNodeSaveData } from "./types.js";
import { validFxNodeReplayCommand } from "./validate.js";

export const FXNODE_SAVE_DATA_LIMITS = Object.freeze({
  maxCommands: 1_000,
  maxAtomicCommands: 10_000,
  maxValues: 100_000,
  maxStringCodeUnits: 1_048_576,
  maxDepth: 50,
  maxIssues: 100,
});
const SAVE_ENVELOPE_OVERHEAD = 256;
export type FxNodeSaveDataDecodeResult<C extends FxNodeCompositionData = FxNodeCompositionData> =
  | { readonly ok: true; readonly value: CompatibleFxNodeSaveData<C>; readonly baseline: GraphDocument<C> }
  | { readonly ok: false; readonly issues: readonly BoundValidationIssue[] };
const exact = (value: Record<string, unknown>, keys: readonly string[]) =>
  Object.keys(value).length === keys.length && keys.every((k) => Object.hasOwn(value, k));
const issue = (code: string, path: string, message: string): readonly BoundValidationIssue[] =>
  deepFreeze([{ code, path, message }]);
const prefix = (path: string) => `/baseline${path === "/" ? "" : path.startsWith("/") ? path : `/${path}`}`;

/** Save-decoder-only bridge for durable command logs that embedded the schema-1 menu catalog. */
const normalizeSavedComposition = (input: unknown): unknown => {
  if (
    !isRecord(input) ||
    input.schemaVersion !== 1 ||
    !Object.hasOwn(input, "menuGroups") ||
    !isRecord(input.menuGroups) ||
    !isRecord(input.nodes)
  )
    return input;
  for (const group of Object.values(input.menuGroups))
    if (
      !isRecord(group) ||
      !exact(group, ["title", "order"]) ||
      typeof group.title !== "string" ||
      group.title.length === 0 ||
      group.title.length > FXNODE_COMPOSITION_LIMITS.maxTitleLength ||
      !Number.isSafeInteger(group.order)
    )
      return input;
  for (const definition of Object.values(input.nodes)) {
    if (!isRecord(definition) || !Object.hasOwn(definition, "menu") || !isRecord(definition.menu)) return input;
    const menu = definition.menu;
    if (menu.kind === "hidden") {
      if (!exact(menu, ["kind"])) return input;
      continue;
    }
    if (
      menu.kind !== "entry" ||
      !exact(menu, ["kind", "group", "order", "keywords"]) ||
      typeof menu.group !== "string" ||
      !Object.hasOwn(input.menuGroups, menu.group) ||
      !Number.isSafeInteger(menu.order) ||
      !Array.isArray(menu.keywords) ||
      menu.keywords.some(
        (keyword) => typeof keyword !== "string" || keyword.length > FXNODE_COMPOSITION_LIMITS.maxKeywordLength,
      )
    )
      return input;
  }
  const { menuGroups: _menuGroups, ...composition } = input;
  return {
    ...composition,
    schemaVersion: 2,
    nodes: Object.fromEntries(
      Object.entries(input.nodes).map(([id, definition]) => {
        if (!isRecord(definition)) return [id, definition];
        const { menu: _menu, ...node } = definition;
        return [id, node];
      }),
    ),
  };
};

/** Bounded, total decoder for an admitted command-log save envelope. */
export function decodeFxNodeSaveData<C extends FxNodeCompositionData>(
  source: unknown,
  composition: { readonly source: C },
  decodeBaseline: (value: unknown) => BoundDecodeResult<C>,
): FxNodeSaveDataDecodeResult<C> {
  try {
    const admitted = admitStructuredData(source, {
      maxValues:
        PERSISTENCE_LIMITS.maxValues +
        FXNODE_SAVE_DATA_LIMITS.maxValues +
        FXNODE_COMPOSITION_LIMITS.maxValues +
        SAVE_ENVELOPE_OVERHEAD,
      maxStringCodeUnits:
        PERSISTENCE_LIMITS.maxStringCodeUnits +
        FXNODE_SAVE_DATA_LIMITS.maxStringCodeUnits +
        FXNODE_COMPOSITION_LIMITS.maxStringCodeUnits +
        SAVE_ENVELOPE_OVERHEAD,
      maxDepth:
        Math.max(PERSISTENCE_LIMITS.maxDepth, FXNODE_SAVE_DATA_LIMITS.maxDepth, FXNODE_COMPOSITION_LIMITS.maxDepth) + 2,
      maxIssues: FXNODE_SAVE_DATA_LIMITS.maxIssues,
    });
    if (!admitted.ok) return { ok: false, issues: deepFreeze(admitted.issues) };
    const value = admitted.value;
    if (!isRecord(value) || value.kind !== "fxnode.command-log")
      return { ok: false, issues: issue("save.shape", "/", "Invalid FxNodeSaveData envelope") };
    if (value.schemaVersion === 1)
      return {
        ok: false,
        issues: issue(
          "save.schema.unsupported",
          "/schemaVersion",
          "FxNodeSaveData schema version 1 is no longer supported",
        ),
      };
    if (Number.isSafeInteger(value.schemaVersion) && Number(value.schemaVersion) > 2)
      return {
        ok: false,
        issues: issue(
          "save.schema.future",
          "/schemaVersion",
          "FxNodeSaveData schema version is newer than this runtime",
        ),
      };
    if (!exact(value, ["kind", "schemaVersion", "composition", "baseline", "commands"]) || value.schemaVersion !== 2)
      return { ok: false, issues: issue("save.shape", "/", "Invalid FxNodeSaveData envelope") };
    const normalizedComposition = normalizeSavedComposition(value.composition);
    const savedComposition = validateFxNodeComposition(normalizedComposition);
    if (!savedComposition.ok)
      return {
        ok: false,
        issues: deepFreeze([
          { code: "save.composition.invalid", path: "/composition", message: "Saved composition is invalid" },
          ...savedComposition.issues.slice(0, 99).map((x) => ({ ...x, path: `/composition${x.path}` })),
        ]),
      };
    if (!Array.isArray(value.commands))
      return { ok: false, issues: issue("save.commands", "/commands", "Commands must be an array") };
    if (value.commands.length > FXNODE_SAVE_DATA_LIMITS.maxCommands)
      return { ok: false, issues: issue("limit.commands", "/commands", "Command limit exceeded") };
    const admittedCommands = admitStructuredData(value.commands, FXNODE_SAVE_DATA_LIMITS);
    if (!admittedCommands.ok)
      return {
        ok: false,
        issues: deepFreeze(admittedCommands.issues.map((x) => ({ ...x, path: `/commands${x.path}` }))),
      };
    if (!isRecord(value.baseline) || value.baseline.schemaVersion !== 2)
      return {
        ok: false,
        issues: issue("baseline.schema", "/baseline/schemaVersion", "Baseline must be a GraphLayoutV2 record"),
      };
    const admittedBaseline = admitStructuredData(value.baseline, PERSISTENCE_LIMITS);
    if (!admittedBaseline.ok)
      return {
        ok: false,
        issues: deepFreeze(admittedBaseline.issues.map((x) => ({ ...x, path: `/baseline${x.path}` }))),
      };
    const compatibility = saveCompositionCompatibility(
      savedComposition.value,
      composition.source,
      value.baseline as unknown as GraphLayoutV2,
    );
    if (compatibility.length) return { ok: false, issues: compatibility };
    let atomic = 0;
    for (let i = 0; i < value.commands.length; i++) {
      const command = value.commands[i];
      if (!validFxNodeReplayCommand<C>(command, (id) => Object.hasOwn(savedComposition.value.nodes, id)))
        return {
          ok: false,
          issues: issue(
            "command.invalid",
            `/commands/${i}`,
            "Invalid replay command or node type is absent from saved composition",
          ),
        };
      atomic += command.type === "batch" ? command.commands.length : 1;
      if (atomic > FXNODE_SAVE_DATA_LIMITS.maxAtomicCommands)
        return { ok: false, issues: issue("limit.atomic-commands", `/commands/${i}`, "Atomic command limit exceeded") };
    }
    const decoded = decodeBaseline(value.baseline);
    if (!decoded.ok)
      return {
        ok: false,
        issues: deepFreeze(
          decoded.issues.slice(0, FXNODE_SAVE_DATA_LIMITS.maxIssues).map((x) => ({ ...x, path: prefix(x.path) })),
        ),
      };
    return {
      ok: true,
      value: deepFreeze({ ...value, composition: savedComposition.value } as unknown as CompatibleFxNodeSaveData<C>),
      baseline: decoded.value,
    };
  } catch {
    return { ok: false, issues: issue("save.decode", "/", "Save data could not be decoded") };
  }
}
