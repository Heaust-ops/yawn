import { isColorRamp } from "../widgets/color-ramp.js";
import type { FxNodeComposition, FxNodeValueSchema, FxNodeVisibility } from "./types.js";

export const FXNODE_COMPOSITION_LIMITS = Object.freeze({
  maxIssues: 100,
  maxDepth: 64,
  maxValues: 100_000,
  maxStringCodeUnits: 1_048_576,
  maxIdLength: 128,
  maxTitleLength: 256,
  maxKeywordLength: 64,
  maxSearchLength: 4096,
  maxNodes: 512,
  maxSocketTypes: 128,
  maxStyles: 128,
  maxResources: 64,
  maxParametersPerNode: 128,
  maxSocketsPerNode: 64,
  maxUiRowsPerNode: 256,
  maxEnumValues: 256,
  maxMigrationsPerNode: 64,
  maxMigrationSteps: 128,
  maxVisibilityDepth: 16,
  maxVisibilityNodes: 256,
  maxImageBytes: 33_554_432,
  maxImageDimension: 8192,
  maxImagePixels: 16_777_216,
});
export interface FxNodeCompositionIssue {
  readonly code: string;
  readonly path: string;
  readonly message: string;
}
export type FxNodeCompositionValidation =
  | { readonly ok: true; readonly value: FxNodeComposition }
  | { readonly ok: false; readonly issues: readonly FxNodeCompositionIssue[] };
const forbidden = new Set(["__proto__", "prototype", "constructor"]);
const esc = (s: string) => s.replaceAll("~", "~0").replaceAll("/", "~1");
const plain = (v: unknown): v is Record<string, unknown> =>
  v !== null &&
  typeof v === "object" &&
  !Array.isArray(v) &&
  (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null);

export function validateFxNodeComposition(input: unknown): FxNodeCompositionValidation {
  const issues: FxNodeCompositionIssue[] = [];
  const add = (path: string, message: string, code = "shape.invalid") => {
    if (issues.length < FXNODE_COMPOSITION_LIMITS.maxIssues) issues.push(Object.freeze({ code, path, message }));
  };
  const seen = new WeakSet<object>();
  let count = 0,
    bytes = 0;
  let terminal = false;
  const stringBudget = (value: string, path: string) => {
    bytes += value.length;
    if (bytes <= FXNODE_COMPOSITION_LIMITS.maxStringCodeUnits) return true;
    add(path, "composition exceeds string limit", "limit.strings");
    terminal = true;
    return false;
  };
  const inspect = (v: unknown, p: string, d: number): boolean => {
    try {
      if (terminal) return false;
      if (++count > FXNODE_COMPOSITION_LIMITS.maxValues) {
        add(p, "composition exceeds value limit", "limit.values");
        terminal = true;
        return false;
      }
      if (d > FXNODE_COMPOSITION_LIMITS.maxDepth) {
        add(p, "composition exceeds depth limit", "limit.depth");
        terminal = true;
        return false;
      }
      if (typeof v === "string") {
        return stringBudget(v, p);
      }
      if (v === null || typeof v === "boolean") return true;
      if (typeof v === "number") {
        if (!Number.isFinite(v)) add(p, "must be finite", "value.finite");
        return true;
      }
      if (typeof v !== "object") {
        add(p, "unsupported value", "data.type");
        return false;
      }
      if (seen.has(v)) {
        add(p, "shared or cyclic value", "data.identity");
        return false;
      }
      seen.add(v);
      if (Array.isArray(v)) {
        if (Object.getPrototypeOf(v) !== Array.prototype) add(p, "array must be plain", "data.array");
        const ds = Object.getOwnPropertyDescriptors(v);
        const ld = Object.getOwnPropertyDescriptor(v, "length");
        const length = ld && "value" in ld ? ld.value : undefined;
        if (!Number.isSafeInteger(length)) {
          add(p, "invalid array length", "data.array");
          return false;
        }
        if (Number(length) > FXNODE_COMPOSITION_LIMITS.maxValues) {
          add(p, "array exceeds value limit", "limit.values");
          terminal = true;
          return false;
        }
        for (const key of Reflect.ownKeys(ds)) {
          if (key === "length") continue;
          if (typeof key === "string" && !stringBudget(key, p)) return false;
          if (typeof key !== "string" || !/^(0|[1-9]\d*)$/.test(key) || Number(key) >= Number(length)) {
            add(p, "extra array own property", "data.array");
            continue;
          }
          const descriptor = ds[key]!;
          if (!descriptor.enumerable || !("value" in descriptor)) {
            add(`${p}/${esc(key)}`, "array elements must be enumerable data properties", "data.inspect");
            continue;
          }
          inspect(descriptor.value, `${p}/${key}`, d + 1);
        }
        for (let i = 0; i < Number(length) && !terminal; i++)
          if (!Object.hasOwn(ds, String(i))) add(`${p}/${i}`, "sparse arrays are not allowed", "data.array");
        return true;
      }
      const proto = Object.getPrototypeOf(v);
      if (proto !== Object.prototype && proto !== null) {
        add(p, "object must be an ordinary record", "data.type");
        return false;
      }
      for (const k of Reflect.ownKeys(v)) {
        if (typeof k !== "string") {
          add(p, "symbol properties are not allowed", "data.symbol");
          continue;
        }
        if (!stringBudget(k, p)) return false;
        const q = `${p}/${esc(k)}`,
          descriptor = Object.getOwnPropertyDescriptor(v, k);
        if (!descriptor || !descriptor.enumerable || !("value" in descriptor)) {
          add(q, "record properties must be enumerable data properties", "data.inspect");
          continue;
        }
        inspect(descriptor.value, q, d + 1);
      }
      return true;
    } catch {
      add(p, "value could not be inspected", "data.inspect");
      terminal = true;
      return false;
    }
  };
  inspect(input, "", 0);
  if (issues.length) return Object.freeze({ ok: false, issues: Object.freeze(issues) });
  let cloned: unknown;
  try {
    cloned = structuredClone(input);
  } catch {
    add("", "value could not be cloned", "data.clone");
    return Object.freeze({ ok: false, issues: Object.freeze(issues) });
  }
  input = cloned;
  if (!plain(input)) {
    add("", "must be a plain object");
    return Object.freeze({ ok: false, issues: Object.freeze(issues) });
  }
  const known = (o: Record<string, unknown>, keys: readonly string[], p: string) => {
    for (const k of Object.keys(o)) {
      if (issues.length >= FXNODE_COMPOSITION_LIMITS.maxIssues) return;
      if (!keys.includes(k)) add(`${p}/${esc(k)}`, "unknown property", "shape.unknown");
    }
  };
  const exact = (o: Record<string, unknown>, keys: readonly string[], p: string) => {
    known(o, keys, p);
    for (const k of keys) {
      if (issues.length >= FXNODE_COMPOSITION_LIMITS.maxIssues) return;
      if (!Object.hasOwn(o, k)) add(`${p}/${k}`, "missing property", "shape.missing");
    }
  };
  const id = (v: unknown, p: string) => {
    if (typeof v !== "string" || !v || v.length > 128 || /[\x00-\x1f\x7f]/.test(v) || forbidden.has(v))
      add(p, "invalid ID");
  };
  const str = (v: unknown, p: string, max: number = FXNODE_COMPOSITION_LIMITS.maxTitleLength) => {
    if (typeof v !== "string" || v.length > max) add(p, "invalid string", "shape.string");
  };
  const integer = (v: unknown, p: string, min = 0) => {
    if (!Number.isSafeInteger(v) || Number(v) < min) add(p, "invalid integer", "shape.integer");
  };
  const record = (
    v: unknown,
    p: string,
    max: number,
    visit: (x: Record<string, unknown>, q: string, key: string) => void,
  ) => {
    if (!plain(v)) {
      add(p, "must be a record", "shape.record");
      return;
    }
    const es = Object.entries(v);
    if (es.length > max) add(p, "too many entries", "limit.collection");
    for (const [k, x] of es) {
      if (issues.length >= FXNODE_COMPOSITION_LIMITS.maxIssues) return;
      id(k, `${p}/${esc(k)}`);
      if (plain(x)) visit(x, `${p}/${esc(k)}`, k);
      else add(`${p}/${esc(k)}`, "must be an object", "shape.object");
    }
  };
  exact(
    input,
    ["schemaVersion", "id", "version", "compatibility", "socketTypes", "nodeStyles", "resources", "theme", "nodes"],
    "",
  );
  if (input.schemaVersion !== 2) add("/schemaVersion", "must equal 2");
  id(input.id, "/id");
  integer(input.version, "/version", 1);
  const socketIds = new Set(Object.keys(plain(input.socketTypes) ? input.socketTypes : {})),
    styleIds = new Set(Object.keys(plain(input.nodeStyles) ? input.nodeStyles : {})),
    resourceIds = new Set(Object.keys(plain(input.resources) ? input.resources : {}));
  if (!plain(input.compatibility)) add("/compatibility", "must be an object");
  else {
    exact(input.compatibility, ["wildcardInputTypes"], "/compatibility");
    const wildcards = input.compatibility.wildcardInputTypes;
    if (!Array.isArray(wildcards)) add("/compatibility/wildcardInputTypes", "must be an array");
    else {
      const seen = new Set<unknown>();
      wildcards.forEach((value, index) => {
        id(value, `/compatibility/wildcardInputTypes/${index}`);
        if (typeof value === "string" && !socketIds.has(value))
          add(`/compatibility/wildcardInputTypes/${index}`, "unknown socket type", "reference.socketType");
        if (seen.has(value))
          add(`/compatibility/wildcardInputTypes/${index}`, "duplicate socket type", "value.duplicate");
        seen.add(value);
      });
    }
  }
  const color = (v: unknown, p: string) => {
    if (typeof v !== "string" || !/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v)) add(p, "invalid color");
  };
  record(input.socketTypes, "/socketTypes", 128, (x, p) => {
    exact(x, ["title", "color", "acceptsFrom"], p);
    str(x.title, p + "/title");
    color(x.color, p + "/color");
    if (!Array.isArray(x.acceptsFrom)) add(p + "/acceptsFrom", "must be an array");
    else {
      const seen = new Set<unknown>();
      x.acceptsFrom.forEach((v, i) => {
        id(v, `${p}/acceptsFrom/${i}`);
        if (seen.has(v)) add(`${p}/acceptsFrom/${i}`, "duplicate socket type", "value.duplicate");
        seen.add(v);
        if (typeof v === "string" && !socketIds.has(v))
          add(`${p}/acceptsFrom/${i}`, "unknown socket type", "reference.socketType");
      });
    }
  });
  record(input.nodeStyles, "/nodeStyles", 128, (x, p) => {
    exact(x, ["header"], p);
    color(x.header, p + "/header");
  });
  record(input.resources, "/resources", FXNODE_COMPOSITION_LIMITS.maxResources, (x, p) => {
    exact(
      x,
      ["kind", "title", "openTitle", "accept", "referencePrefix", "maxBytes", "maxWidth", "maxHeight", "maxPixels"],
      p,
    );
    if (x.kind !== "image") add(p + "/kind", "must equal image", "shape.literal");
    str(x.title, p + "/title");
    str(x.openTitle, p + "/openTitle");
    if (
      typeof x.referencePrefix !== "string" ||
      !x.referencePrefix ||
      x.referencePrefix.length > 128 ||
      /[\x00-\x1f\x7f]/.test(x.referencePrefix) ||
      !x.referencePrefix.endsWith(":")
    )
      add(p + "/referencePrefix", "invalid reference prefix", "shape.string");
    if (
      !Array.isArray(x.accept) ||
      !x.accept.length ||
      x.accept.some((v) => typeof v !== "string" || !v || v.length > FXNODE_COMPOSITION_LIMITS.maxKeywordLength)
    )
      add(p + "/accept", "invalid accepted media array", "shape.array");
    else {
      const s = new Set(x.accept);
      if (s.size !== x.accept.length) add(p + "/accept", "duplicate media type", "value.duplicate");
    }
    for (const k of ["maxBytes", "maxWidth", "maxHeight", "maxPixels"] as const)
      if (!Number.isSafeInteger(x[k]) || Number(x[k]) < 1)
        add(`${p}/${k}`, "must be a positive safe integer", "limit.resource");
  });
  const themeKeys = [
    "background",
    "grid",
    "frame",
    "frameHeader",
    "body",
    "control",
    "controlFill",
    "controlEditing",
    "textSelection",
    "outline",
    "text",
    "muted",
    "shadow",
    "nodeSelected",
    "nodeActive",
    "unknownHeader",
    "unknownSocket",
    "linkMuted",
    "knifeMuted",
    "emphasis",
    "focus",
    "editOutline",
    "resize",
    "muteOverlay",
    "boxSelectionFill",
    "checkerLight",
    "checkerDark",
    "widgetBorder",
    "rampBorder",
    "resourceBackground",
  ] as const;
  if (!plain(input.theme)) add("/theme", "must be an object");
  else {
    exact(input.theme, themeKeys, "/theme");
    for (const k of themeKeys) color(input.theme[k], `/theme/${k}`);
  }
  const schema = (x: unknown, p: string) => {
    if (!plain(x)) {
      add(p, "must be a schema", "schema.shape");
      return;
    }
    const t = x.type;
    if (typeof t !== "string" || !["number", "string", "boolean", "vector", "color", "json"].includes(t))
      add(p + "/type", "unknown schema type", "schema.type");
    const allowed =
      t === "number"
        ? ["type", "default", "integer", "minimum", "maximum", "softMin", "softMax", "step", "precision"]
        : t === "string"
          ? ["type", "default", "enum"]
          : t === "vector" || t === "color"
            ? ["type", "default", "minimum", "maximum", "softMin", "softMax", "step"]
            : t === "json"
              ? ["type", "default", "codec"]
              : ["type", "default"];
    exact(
      x,
      allowed.filter((k) => k === "type" || k === "default" || Object.hasOwn(x, k)),
      p,
    );
    if (t === "number" && x.integer !== undefined && typeof x.integer !== "boolean")
      add(p + "/integer", "integer must be boolean", "schema.integer");
    const d = x.default;
    if (!plain(d) || d.kind !== t) {
      add(p + "/default", "tagged default does not match schema", "schema.default");
    } else {
      exact(d, ["kind", "value"], p + "/default");
      const val = d.value;
      if (t === "number" && (!Number.isFinite(val) || (x.integer === true && !Number.isSafeInteger(val))))
        add(p + "/default/value", "invalid number", "schema.default");
      if (t === "string" && typeof val !== "string") add(p + "/default/value", "invalid string", "schema.default");
      if (t === "boolean" && typeof val !== "boolean") add(p + "/default/value", "invalid boolean", "schema.default");
      if (t === "vector" || t === "color") {
        const n = t === "vector" ? 3 : 4;
        if (!Array.isArray(val) || val.length !== n || val.some((v) => !Number.isFinite(v)))
          add(p + "/default/value", "invalid components", "schema.default");
        else if (
          val.some(
            (v) => (typeof x.minimum === "number" && v < x.minimum) || (typeof x.maximum === "number" && v > x.maximum),
          )
        )
          add(p + "/default/value", "component outside bounds", "schema.bounds");
      }
      if (t === "json" && x.codec === "color-ramp/v1" && !isColorRamp(val))
        add(p + "/default/value", "invalid color ramp", "schema.codec");
      if (
        typeof val === "number" &&
        ((typeof x.minimum === "number" && val < x.minimum) || (typeof x.maximum === "number" && val > x.maximum))
      )
        add(p + "/default/value", "default outside bounds", "schema.bounds");
    }
    for (const k of ["minimum", "maximum", "softMin", "softMax"] as const)
      if (x[k] !== undefined && !Number.isFinite(x[k])) add(`${p}/${k}`, "must be finite", "schema.bounds");
    if (typeof x.minimum === "number" && typeof x.maximum === "number" && x.minimum > x.maximum)
      add(p + "/minimum", "minimum exceeds maximum", "schema.bounds");
    if (typeof x.softMin === "number" && typeof x.softMax === "number" && x.softMin > x.softMax)
      add(p + "/softMin", "soft minimum exceeds soft maximum", "schema.bounds");
    if (typeof x.softMin === "number" && typeof x.minimum === "number" && x.softMin < x.minimum)
      add(p + "/softMin", "soft minimum is outside hard bounds", "schema.bounds");
    if (typeof x.softMax === "number" && typeof x.maximum === "number" && x.softMax > x.maximum)
      add(p + "/softMax", "soft maximum is outside hard bounds", "schema.bounds");
    if (x.step !== undefined && (!Number.isFinite(x.step) || Number(x.step) <= 0))
      add(p + "/step", "step must be positive", "schema.step");
    if (
      x.precision !== undefined &&
      (!Number.isSafeInteger(x.precision) || Number(x.precision) < 0 || Number(x.precision) > 20)
    )
      add(p + "/precision", "invalid precision", "schema.precision");
    if (t === "json" && x.codec !== undefined && x.codec !== "color-ramp/v1")
      add(p + "/codec", "unknown codec", "schema.codec");
    if (x.enum !== undefined) {
      if (!Array.isArray(x.enum) || !x.enum.length || x.enum.some((v) => typeof v !== "string"))
        add(p + "/enum", "invalid enum", "schema.enum");
      else {
        if (x.enum.length > FXNODE_COMPOSITION_LIMITS.maxEnumValues)
          add(p + "/enum", "too many enum values", "limit.enum");
        if (new Set(x.enum).size !== x.enum.length) add(p + "/enum", "duplicate enum value", "value.duplicate");
        if (plain(d) && typeof d.value === "string" && !x.enum.includes(d.value))
          add(p + "/default/value", "default is not an enum member", "schema.enum");
      }
    }
  };
  const primitive = (s: Record<string, unknown> | undefined, v: unknown) =>
    s?.type === "number"
      ? typeof v === "number" && Number.isFinite(v)
      : s?.type === "string"
        ? typeof v === "string" && (!Array.isArray(s.enum) || s.enum.includes(v))
        : s?.type === "boolean"
          ? typeof v === "boolean"
          : false;
  const visibility = (
    v: unknown,
    p: string,
    paramDefs: Record<string, unknown>,
    depth: number,
    state: { n: number },
  ) => {
    if (
      ++state.n > FXNODE_COMPOSITION_LIMITS.maxVisibilityNodes ||
      depth > FXNODE_COMPOSITION_LIMITS.maxVisibilityDepth
    ) {
      add(p, "visibility expression too complex", "limit.visibility");
      return;
    }
    if (!plain(v)) {
      add(p, "invalid visibility expression", "ui.visibility");
      return;
    }
    if ("parameter" in v) {
      const hasEquals = Object.hasOwn(v, "equals"),
        hasIn = Object.hasOwn(v, "in");
      if (hasEquals === hasIn) {
        add(p, "visibility requires exactly equals or in", "ui.visibility");
        return;
      }
      exact(v, ["parameter", hasEquals ? "equals" : "in"], p);
      const def = typeof v.parameter === "string" && plain(paramDefs[v.parameter]) ? paramDefs[v.parameter] : undefined;
      if (!def) add(p + "/parameter", "unknown parameter", "reference.parameter");
      const values = hasEquals ? [v.equals] : v.in;
      if (!hasEquals && (!Array.isArray(values) || !values.length)) {
        add(p + "/in", "must be a nonempty array", "ui.visibility");
        return;
      }
      if (Array.isArray(values))
        values.forEach((z, i) => {
          if (!primitive(def as Record<string, unknown> | undefined, z))
            add(`${p}/${hasEquals ? "equals" : `in/${i}`}`, "comparison does not match schema", "ui.visibility");
        });
      return;
    }
    const hasAll = Object.hasOwn(v, "all"),
      hasAny = Object.hasOwn(v, "any");
    if (hasAll === hasAny) {
      add(p, "visibility requires exactly all or any", "ui.visibility");
      return;
    }
    const op = hasAll ? "all" : "any";
    exact(v, [op], p);
    if (!Array.isArray(v[op]) || v[op].length === 0) add(p + `/${op}`, "must be a nonempty array", "ui.visibility");
    else
      for (let i = 0; i < v[op].length && issues.length < FXNODE_COMPOSITION_LIMITS.maxIssues; i++)
        visibility(v[op][i], `${p}/${op}/${i}`, paramDefs, depth + 1, state);
  };
  record(input.nodes, "/nodes", FXNODE_COMPOSITION_LIMITS.maxNodes, (node, p) => {
    exact(
      node,
      ["version", "title", "behavior", "style", "parameters", "sockets", "ui", "muteBypass", "migrations"],
      p,
    );
    integer(node.version, p + "/version", 1);
    str(node.title, p + "/title");
    if (typeof node.behavior !== "string" || !["standard", "frame", "reroute"].includes(node.behavior))
      add(p + "/behavior", "invalid behavior", "shape.literal");
    if (typeof node.style !== "string" || !styleIds.has(node.style))
      add(p + "/style", "unknown node style", "reference.style");
    const paramDefs = plain(node.parameters) ? node.parameters : {};
    const socketDefs = plain(node.sockets) ? node.sockets : {};
    const params = new Set(Object.keys(paramDefs)),
      sockets = new Set(Object.keys(socketDefs));
    record(node.parameters, p + "/parameters", FXNODE_COMPOSITION_LIMITS.maxParametersPerNode, (x, q) => schema(x, q));
    record(node.sockets, p + "/sockets", FXNODE_COMPOSITION_LIMITS.maxSocketsPerNode, (x, q) => {
      exact(x, ["title", "direction", "type", "maxIncomingLinks", "visible", "value", "showValue"], q);
      str(x.title, q + "/title");
      if (typeof x.direction !== "string" || !["input", "output"].includes(x.direction))
        add(q + "/direction", "invalid direction", "socket.direction");
      if (typeof x.type !== "string" || !socketIds.has(x.type))
        add(q + "/type", "unknown socket type", "reference.socketType");
      integer(x.maxIncomingLinks, q + "/maxIncomingLinks");
      if (typeof x.visible !== "boolean" || typeof x.showValue !== "boolean")
        add(q, "visibility flags must be boolean", "socket.flags");
      if (x.value !== null) schema(x.value, q + "/value");
      if (x.direction === "input" && Number(x.maxIncomingLinks) <= 0)
        add(q + "/maxIncomingLinks", "input requires a positive limit", "socket.links");
      if (x.direction === "output" && (x.maxIncomingLinks !== 0 || x.value !== null || x.showValue !== false))
        add(q, "output payload must be null and hidden", "socket.output");
      if (x.showValue === true && x.value === null) add(q + "/value", "shown sockets require a value", "socket.value");
    });
    const placements = new Map<string, string>(),
      visState = { n: 0 };
    const place = (key: string, q: string) => {
      const old = placements.get(key);
      if (old) add(q, "duplicate or conflicting UI placement", "ui.placement");
      else placements.set(key, q);
    };
    if (!Array.isArray(node.ui)) add(p + "/ui", "must be an array", "ui.shape");
    else {
      if (node.ui.length > FXNODE_COMPOSITION_LIMITS.maxUiRowsPerNode) add(p + "/ui", "too many rows", "limit.ui");
      for (let i = 0; i < node.ui.length && issues.length < FXNODE_COMPOSITION_LIMITS.maxIssues; i++) {
        const r = node.ui[i],
          q = `${p}/ui/${i}`;
        if (!plain(r)) {
          add(q, "invalid UI row", "ui.shape");
          continue;
        }
        const common = r.visibleWhen !== undefined ? ["visibleWhen"] : [];
        let req: string[] = [];
        if (r.kind === "parameter") req = ["kind", "parameter", ...("title" in r ? ["title"] : []), ...common];
        else if (r.kind === "socket") req = ["kind", "socket", ...("title" in r ? ["title"] : []), ...common];
        else if (r.kind === "widget" && r.widget === "color-ramp")
          req = ["kind", "widget", "parameter", ...("title" in r ? ["title"] : []), ...common];
        else if (r.kind === "widget" && r.widget === "grading-wheels") req = ["kind", "widget", "bindings", ...common];
        else if (r.kind === "widget") {
          add(q + "/widget", "unknown widget", "ui.widget");
          continue;
        } else if (r.kind === "resource")
          req = [
            "kind",
            "resource",
            "parameter",
            ...("title" in r ? ["title"] : []),
            ...("openTitle" in r ? ["openTitle"] : []),
            ...common,
          ];
        else if (r.kind === "text") req = ["kind", "variant", "title", ...common];
        else if (r.kind === "hidden" && (r.target === "parameter" || r.target === "socket"))
          req = ["kind", "target", r.target === "socket" ? "socket" : "parameter"];
        else {
          if (r.kind === "hidden") add(q + "/target", "invalid hidden target", "ui.target");
          else add(q + "/kind", "unknown UI variant", "ui.kind");
          continue;
        }
        exact(r, req, q);
        if ("title" in r) str(r.title, q + "/title");
        if ("openTitle" in r) str(r.openTitle, q + "/openTitle");
        if (
          r.kind === "text" &&
          (typeof r.variant !== "string" ||
            !["header", "category", "section", "panel", "placeholder"].includes(r.variant))
        )
          add(q + "/variant", "invalid text variant", "ui.text");
        for (const field of ["parameter", "socket", "resource"])
          if (req.includes(field) && typeof r[field] !== "string")
            add(q + `/${field}`, "must be a string", "shape.string");
        if (typeof r.parameter === "string") {
          if (!params.has(r.parameter)) add(q + "/parameter", "unknown parameter", "reference.parameter");
          else if (r.kind !== "hidden" || r.target === "parameter") place(`p:${r.parameter}`, q);
        }
        if (typeof r.socket === "string") {
          if (!sockets.has(r.socket)) add(q + "/socket", "unknown socket", "reference.socket");
          else place(`s:${r.socket}`, q);
        }
        if (typeof r.resource === "string" && !resourceIds.has(r.resource))
          add(q + "/resource", "unknown resource", "reference.resource");
        if (r.kind === "widget" && r.widget === "color-ramp") {
          const d = plain(paramDefs[String(r.parameter)])
            ? (paramDefs[String(r.parameter)] as Record<string, unknown>)
            : undefined;
          if (d?.type !== "json" || d.codec !== "color-ramp/v1")
            add(q + "/parameter", "color ramp requires matching codec", "ui.widget");
        }
        if (r.kind === "resource") {
          const d = plain(paramDefs[String(r.parameter)])
            ? (paramDefs[String(r.parameter)] as Record<string, unknown>)
            : undefined;
          if (d?.type !== "string") add(q + "/parameter", "resource requires string schema", "ui.resource");
        }
        if (r.widget === "grading-wheels") {
          if (!Array.isArray(r.bindings) || r.bindings.length !== 3)
            add(q + "/bindings", "grading wheels require exactly 3 bindings", "ui.widget");
          if (Array.isArray(r.bindings))
            r.bindings.forEach((b, j) => {
              const z = `${q}/bindings/${j}`;
              if (!plain(b)) {
                add(z, "invalid binding", "ui.widget");
                return;
              }
              exact(b, ["title", "scalar", "color"], z);
              str(b.title, z + "/title");
              const sd = plain(paramDefs[String(b.scalar)])
                  ? (paramDefs[String(b.scalar)] as Record<string, unknown>)
                  : undefined,
                cd = plain(paramDefs[String(b.color)])
                  ? (paramDefs[String(b.color)] as Record<string, unknown>)
                  : undefined;
              if (sd?.type !== "number") add(z + "/scalar", "scalar requires number schema", "ui.widget");
              if (cd?.type !== "color") add(z + "/color", "color requires color schema", "ui.widget");
              if (b.scalar === b.color) add(z, "bindings must differ", "ui.widget");
              if (typeof b.scalar === "string" && params.has(b.scalar)) place(`p:${b.scalar}`, z);
              if (typeof b.color === "string" && params.has(b.color)) place(`p:${b.color}`, z);
            });
        }
        if (r.visibleWhen !== undefined) visibility(r.visibleWhen, q + "/visibleWhen", paramDefs, 0, visState);
      }
    }
    for (const key of params)
      if (!placements.has(`p:${key}`))
        add(`${p}/parameters/${esc(key)}`, "parameter has no UI placement", "ui.placement");
    for (const key of sockets)
      if (!placements.has(`s:${key}`)) add(`${p}/sockets/${esc(key)}`, "socket has no UI placement", "ui.placement");
    if (!Array.isArray(node.muteBypass)) add(p + "/muteBypass", "must be an array", "shape.array");
    else {
      const pairs = new Set<string>();
      node.muteBypass.forEach((b, i) => {
        const q = `${p}/muteBypass/${i}`;
        if (!Array.isArray(b) || b.length !== 2 || b.some((x) => typeof x !== "string" || !sockets.has(x)))
          add(q, "invalid socket bypass", "reference.socket");
        else {
          const pair = `${b[0]}\0${b[1]}`;
          if (pairs.has(pair)) add(q, "duplicate bypass", "value.duplicate");
          pairs.add(pair);
          const a = socketDefs[b[0]] as Record<string, unknown>,
            z = socketDefs[b[1]] as Record<string, unknown>,
            dest =
              plain(input.socketTypes) && plain(input.socketTypes[String(a?.type)])
                ? (input.socketTypes[String(a?.type)] as Record<string, unknown>)
                : undefined;
          if (a?.direction !== "input" || z?.direction !== "output")
            add(q, "bypass direction is invalid", "socket.bypass");
          else if (
            !Array.isArray(dest?.acceptsFrom) ||
            (!(
              plain(input.compatibility) &&
              Array.isArray(input.compatibility.wildcardInputTypes) &&
              input.compatibility.wildcardInputTypes.includes(a.type)
            ) &&
              !dest.acceptsFrom.includes(z.type))
          )
            add(q, "incompatible bypass", "socket.compatibility");
        }
      });
    }
    if (!Array.isArray(node.migrations)) add(p + "/migrations", "must be an array", "migration.shape");
    else {
      if (node.migrations.length > FXNODE_COMPOSITION_LIMITS.maxMigrationsPerNode)
        add(p + "/migrations", "too many migrations", "limit.migrations");
      const outgoing = new Set<unknown>();
      node.migrations.forEach((m, i) => {
        const q = `${p}/migrations/${i}`;
        if (!plain(m)) {
          add(q, "invalid migration", "migration.shape");
          return;
        }
        exact(m, ["fromVersion", "toVersion", "steps"], q);
        integer(m.fromVersion, q + "/fromVersion", 1);
        integer(m.toVersion, q + "/toVersion", 1);
        if (outgoing.has(m.fromVersion)) add(q + "/fromVersion", "duplicate outgoing migration", "migration.outgoing");
        outgoing.add(m.fromVersion);
        if (Number(m.fromVersion) >= Number(m.toVersion) || Number(m.toVersion) > Number(node.version))
          add(q, "invalid migration version edge", "migration.edge");
        if (!Array.isArray(m.steps)) add(q + "/steps", "must be array", "migration.shape");
        else {
          const renameSources = new Set<string>();
          const renameDestinations = new Set<string>();
          const parameterWriters = new Set<string>();
          const socketWriters = new Set<string>();
          const materializedParameters = new Set<string>();
          if (m.steps.length > FXNODE_COMPOSITION_LIMITS.maxMigrationSteps)
            add(q + "/steps", "too many steps", "limit.migrations");
          m.steps.forEach((s, j) => {
            if (!plain(s)) {
              add(`${q}/steps/${j}`, "invalid step", "migration.shape");
              return;
            }
            const props =
              s.kind === "rename-parameter" || s.kind === "rename-socket"
                ? ["kind", "from", "to"]
                : s.kind === "materialize-missing"
                  ? ["kind", "target", "key"]
                  : s.kind === "migrate-parameter"
                    ? ["kind", "parameter", "codec"]
                    : [];
            const z = `${q}/steps/${j}`;
            if (!props.length) {
              add(z + "/kind", "unknown migration step", "migration.kind");
              return;
            }
            exact(s, props, z);
            if (s.kind === "rename-parameter" || s.kind === "rename-socket") {
              const domain = s.kind === "rename-parameter" ? "parameter" : "socket";
              const source = `${domain}:${String(s.from)}`;
              const destination = `${domain}:${String(s.to)}`;
              if (s.from === s.to || renameSources.has(source) || renameDestinations.has(destination))
                add(z + "/from", "conflicting rename source or destination", "migration.rename");
              renameSources.add(source);
              renameDestinations.add(destination);
              const writers = domain === "parameter" ? parameterWriters : socketWriters;
              if (writers.has(String(s.to)))
                add(z + "/to", "multiple migration steps write this target", "migration.write");
              writers.add(String(s.to));
              id(s.from, z + "/from");
              const targets = s.kind === "rename-parameter" ? params : sockets;
              if (typeof s.to !== "string" || !targets.has(s.to))
                add(z + "/to", "unknown current target", "reference.migrationTarget");
            } else if (s.kind === "materialize-missing") {
              const targets = s.target === "parameter" ? params : s.target === "socket" ? sockets : null;
              if (!targets) add(z + "/target", "invalid target", "migration.target");
              else if (typeof s.key !== "string" || !targets.has(s.key))
                add(z + "/key", "unknown current target", "reference.migrationTarget");
              else {
                const writers = s.target === "parameter" ? parameterWriters : socketWriters;
                if (writers.has(s.key))
                  add(z + "/key", "multiple migration steps write this target", "migration.write");
                writers.add(s.key);
                if (s.target === "parameter") materializedParameters.add(s.key);
              }
            } else {
              if (typeof s.parameter === "string") {
                if (parameterWriters.has(s.parameter) && !materializedParameters.has(s.parameter))
                  add(z + "/parameter", "multiple migration steps write this target", "migration.write");
                parameterWriters.add(s.parameter);
              }
              if (typeof s.parameter !== "string" || !params.has(s.parameter))
                add(z + "/parameter", "unknown parameter", "reference.parameter");
              else {
                const target = plain(paramDefs[s.parameter])
                  ? (paramDefs[s.parameter] as Record<string, unknown>)
                  : undefined;
                if (target?.type !== "json" || target.codec !== "color-ramp/v1")
                  add(z + "/parameter", "migration requires a color-ramp/v1 json target", "migration.codec");
              }
              if (s.codec !== "color-ramp/legacy-stops")
                add(z + "/codec", "unknown migration codec", "migration.codec");
            }
          });
        }
      });
    }
  });
  return issues.length
    ? Object.freeze({ ok: false as const, issues: Object.freeze(issues) })
    : Object.freeze({
        ok: true as const,
        value: input as unknown as FxNodeComposition,
      });
}
