import type { JsonValue } from "./types.js";

export interface StructuredDataLimits {
  readonly maxValues: number;
  readonly maxStringCodeUnits: number;
  readonly maxDepth: number;
  readonly maxIssues: number;
}
export interface StructuredDataIssue {
  readonly code: string;
  readonly path: string;
  readonly message: string;
}
export interface StructuredDataMetrics {
  readonly values: number;
  readonly stringCodeUnits: number;
  readonly depth: number;
}
export type StructuredDataAdmissionResult =
  | { readonly ok: true; readonly value: unknown; readonly metrics: StructuredDataMetrics }
  | { readonly ok: false; readonly issues: readonly StructuredDataIssue[] };

/** Inspects hostile input without invoking accessors and returns a detached structured clone. */
export function admitStructuredData(input: unknown, limits: StructuredDataLimits): StructuredDataAdmissionResult {
  const issues: StructuredDataIssue[] = [],
    seen = new WeakSet<object>();
  let values = 0,
    strings = 0,
    maxDepth = 0,
    terminal = false;
  const add = (code: string, path: string, message: string) => {
    if (issues.length < limits.maxIssues) issues.push({ code, path, message });
  };
  const walk = (v: unknown, path: string, depth: number): void => {
    try {
      maxDepth = Math.max(maxDepth, depth);
      if (terminal) return;
      if (++values > limits.maxValues) {
        add("limit.values", path, "Document exceeds inspected value limit");
        terminal = true;
        return;
      }
      if (depth > limits.maxDepth) {
        add("limit.depth", path, "Document exceeds JSON depth limit");
        terminal = true;
        return;
      }
      if (typeof v === "string") {
        strings += v.length;
        if (strings > limits.maxStringCodeUnits) {
          add("limit.strings", path, "Document exceeds string limit");
          terminal = true;
        }
        return;
      }
      if (v === null || typeof v === "boolean") return;
      if (typeof v === "number") {
        if (!Number.isFinite(v)) add("value.finite", path, "Numbers must be finite");
        return;
      }
      if (typeof v !== "object") {
        add("data.type", path, "Unsupported value");
        return;
      }
      if (seen.has(v)) {
        add("data.identity", path, "Shared or cyclic identity is unsupported");
        return;
      }
      seen.add(v);
      if (Array.isArray(v)) {
        if (Object.getPrototypeOf(v) !== Array.prototype) add("data.array", path, "Array must be ordinary");
        const ds = Object.getOwnPropertyDescriptors(v),
          ld = Object.getOwnPropertyDescriptor(v, "length"),
          length = ld && "value" in ld ? ld.value : undefined;
        if (!Number.isSafeInteger(length)) {
          add("data.array", path, "Invalid array length");
          return;
        }
        if (Number(length) > limits.maxValues - values) {
          add("limit.values", path, "Document exceeds inspected value limit");
          terminal = true;
          return;
        }
        for (let i = 0; i < Number(length); i++) {
          if (terminal) break;
          const d = ds[String(i)];
          if (!d) {
            add("data.array", `${path}/${i}`, "Sparse arrays are unsupported");
            continue;
          }
          if (!d.enumerable || !("value" in d)) {
            add("data.inspect", `${path}/${i}`, "Element must be an enumerable data property");
            continue;
          }
          walk(d.value, `${path}/${i}`, depth + 1);
        }
        for (const k of Reflect.ownKeys(ds)) {
          if (terminal) break;
          if (k !== "length" && !(typeof k === "string" && /^(0|[1-9]\d*)$/.test(k) && Number(k) < Number(length)))
            add(typeof k === "symbol" ? "data.symbol" : "data.array", path, "Extra array properties are unsupported");
        }
        return;
      }
      const proto = Object.getPrototypeOf(v);
      if (proto !== Object.prototype && proto !== null) {
        add("data.type", path, "Object must be an ordinary record");
        return;
      }
      for (const k of Reflect.ownKeys(v)) {
        if (typeof k !== "string") {
          add("data.symbol", path, "Symbol properties are unsupported");
          continue;
        }
        strings += k.length;
        if (strings > limits.maxStringCodeUnits) {
          add("limit.strings", path, "Document exceeds string limit");
          terminal = true;
          return;
        }
        const d = Object.getOwnPropertyDescriptor(v, k);
        if (!d || !d.enumerable || !("value" in d)) {
          add("data.inspect", `${path}/${k}`, "Property must be enumerable data");
          continue;
        }
        walk(d.value, `${path}/${k}`, depth + 1);
      }
    } catch {
      add("data.inspect", path, "Value could not be inspected");
      terminal = true;
    }
  };
  walk(input, "", 0);
  if (issues.length) return { ok: false, issues };
  try {
    return {
      ok: true,
      value: structuredClone(input),
      metrics: Object.freeze({ values, stringCodeUnits: strings, depth: maxDepth }),
    };
  } catch {
    return { ok: false, issues: [{ code: "data.clone", path: "/", message: "Document could not be cloned" }] };
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isJson(value: unknown, depth = 0): value is JsonValue {
  if (depth > 50) return false;
  if (value === null || typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) return value.every((item) => isJson(item, depth + 1));
  return isRecord(value) && Object.values(value).every((item) => isJson(item, depth + 1));
}

export function deepFreeze<T>(value: T): T {
  if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) deepFreeze(child);
    Object.freeze(value);
  }
  return value;
}

export function cloneJson<T>(value: T): T {
  return deepFreeze(structuredClone(value));
}

export function nullRecord<T>(entries: Iterable<readonly [string, T]> = []): Readonly<Record<string, T>> {
  const result = Object.create(null) as Record<string, T>;
  for (const [key, value] of entries) result[key] = value;
  return Object.freeze(result);
}

export function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (isRecord(value)) {
    const result = Object.create(null) as Record<string, unknown>;
    for (const key of Object.keys(value).sort()) result[key] = canonicalize(value[key]);
    return result;
  }
  return value;
}

/** Canonical structural equality for admitted JSON-shaped values. */
export function canonicalJsonEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(canonicalize(left)) === JSON.stringify(canonicalize(right));
}
