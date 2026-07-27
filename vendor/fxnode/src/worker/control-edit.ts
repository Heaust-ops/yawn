import type { FxNodeValueSchema } from "../composition/types.js";
import type { ParameterValue } from "../core/types.js";
import type { LayoutControl } from "../layout/types.js";

export function clampNumber(value: number, schema: FxNodeValueSchema | undefined): number {
  if (!schema) return value;
  if (schema.type !== "number" && schema.type !== "vector" && schema.type !== "color") return value;
  const minimum = schema.type === "color" ? 0 : schema.minimum;
  const maximum = schema.type === "color" ? 1 : schema.maximum;
  return Math.min(maximum ?? Infinity, Math.max(minimum ?? -Infinity, value));
}

export function snapNumber(value: number, schema: FxNodeValueSchema | undefined): number {
  if (schema?.type !== "number") return value;
  const step = schema.step ?? (schema.integer ? 1 : undefined);
  if (!step) return value;
  const origin = schema.minimum ?? 0;
  return origin + Math.round((value - origin) / step) * step;
}

export function scrubValue(
  control: LayoutControl,
  original: ParameterValue,
  component: number,
  deltaPixels: number,
  fine: boolean,
  snapping: boolean,
): ParameterValue {
  const scale = fine ? 0.01 : 0.1;
  const next = (value: number): number => {
    const changed = value + deltaPixels * scale;
    const snapped = snapping ? snapNumber(changed, control.schema) : changed;
    const clamped = clampNumber(snapped, control.schema);
    return control.schema?.type === "number" && control.schema.integer ? Math.round(clamped) : clamped;
  };
  if (original.kind === "number") return { kind: "number", value: next(original.value) };
  if (original.kind === "vector") {
    const value: [number, number, number] = [...original.value];
    value[component] = next(value[component] ?? 0);
    return { kind: "vector", value };
  }
  if (original.kind === "color") {
    const value: [number, number, number, number] = [...original.value];
    value[component] = Math.min(1, Math.max(0, next(value[component] ?? 0)));
    return { kind: "color", value };
  }
  return original;
}

export function setNumericComponent(
  control: LayoutControl,
  original: ParameterValue,
  component: number,
  input: number,
): ParameterValue {
  const clamped = clampNumber(input, control.schema);
  const next = control.schema?.type === "number" && control.schema.integer ? Math.round(clamped) : clamped;
  if (original.kind === "number") return { kind: "number", value: next };
  if (original.kind === "vector") {
    const value: [number, number, number] = [...original.value];
    value[component] = next;
    return { kind: "vector", value };
  }
  if (original.kind === "color") {
    const value: [number, number, number, number] = [...original.value];
    value[component] = Math.min(1, Math.max(0, next));
    return { kind: "color", value };
  }
  return original;
}

export function numericStep(control: LayoutControl, fine: boolean): number {
  const base = control.schema?.type === "number" ? (control.schema.step ?? (control.schema.integer ? 1 : 0.1)) : 0.1;
  return fine && control.schema?.type === "number" && !control.schema.integer ? base / 10 : fine ? base / 10 : base;
}

export function cycleEnum(values: readonly string[], current: string, direction: 1 | -1): string {
  const index = values.indexOf(current);
  return values[(Math.max(0, index) + direction + values.length) % values.length] ?? current;
}
