import { isColorRamp } from "../widgets/color-ramp.js";
import { isJson, isRecord } from "../core/json.js";
import type { FxNodeValueSchema } from "./types.js";

const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
/** Matches a persisted tagged value against a composition-owned value schema. */
export function matchesFxNodeValueSchema(schema: FxNodeValueSchema, value: unknown): boolean {
  if (!isRecord(value) || value.kind !== schema.type || !("value" in value)) return false;
  const v = value.value;
  if (schema.type === "number")
    return (
      finite(v) &&
      (!schema.integer || Number.isSafeInteger(v)) &&
      (schema.minimum === undefined || v >= schema.minimum) &&
      (schema.maximum === undefined || v <= schema.maximum)
    );
  if (schema.type === "string") return typeof v === "string" && (!schema.enum || schema.enum.includes(v));
  if (schema.type === "boolean") return typeof v === "boolean";
  if (schema.type === "vector" || schema.type === "color") {
    const n = schema.type === "vector" ? 3 : 4;
    return (
      Array.isArray(v) &&
      v.length === n &&
      v.every(
        (x) =>
          finite(x) &&
          (schema.minimum === undefined || x >= schema.minimum) &&
          (schema.maximum === undefined || x <= schema.maximum),
      )
    );
  }
  return schema.codec === "color-ramp/v1" ? isColorRamp(v) : isJson(v);
}
