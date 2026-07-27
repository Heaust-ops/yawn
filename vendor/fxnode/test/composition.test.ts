import assert from "node:assert/strict";
import test from "node:test";
import {
  compileFxNodeComposition,
  composeNode,
  composeSocket,
  FXNODE_COMPOSITION_LIMITS,
  FxNodeCompositionError,
  removeNode,
  removeSocket,
  setTheme,
  validateFxNodeComposition,
  type FxNodeCompositionSeed,
} from "@lib/composition/index.js";

const theme = {
  background: "#112233",
  grid: "#112233",
  frame: "#112233",
  frameHeader: "#112233",
  body: "#112233",
  control: "#112233",
  controlFill: "#112233",
  controlEditing: "#112233",
  textSelection: "#112233",
  outline: "#112233",
  text: "#112233",
  muted: "#112233",
  shadow: "#112233",
  nodeSelected: "#112233",
  nodeActive: "#112233",
  unknownHeader: "#112233",
  unknownSocket: "#112233",
  linkMuted: "#112233",
  knifeMuted: "#112233",
  emphasis: "#112233",
  focus: "#112233",
  editOutline: "#112233",
  resize: "#112233",
  muteOverlay: "#112233",
  boxSelectionFill: "#112233",
  checkerLight: "#112233",
  checkerDark: "#112233",
  widgetBorder: "#112233",
  rampBorder: "#112233",
  resourceBackground: "#112233",
} as const;
const socket = (
  direction: "input" | "output",
  type: "scalar" | "wide",
  value: null | { type: "number"; default: { kind: "number"; value: number } } = null,
) => ({
  title: direction,
  direction,
  type,
  maxIncomingLinks: direction === "input" ? 1 : 0,
  visible: true,
  value: direction === "input" ? value : null,
  showValue: direction === "input" && value !== null,
});
const valid = () =>
  ({
    schemaVersion: 2,
    id: "studio",
    version: 4,
    compatibility: { wildcardInputTypes: [] },
    theme,
    socketTypes: {
      scalar: { title: "Scalar", color: "#abcdef", acceptsFrom: ["scalar"] },
      wide: { title: "Wide", color: "#fedcba", acceptsFrom: ["wide", "scalar"] },
    },
    nodeStyles: { grade: { header: "#123456" }, utility: { header: "#654321" } },
    resources: {
      image: {
        kind: "image",
        title: "Image",
        openTitle: "Open",
        accept: ["image/png", "image/jpeg"],
        referencePrefix: "test-image:",
        maxBytes: 1024,
        maxWidth: 64,
        maxHeight: 64,
        maxPixels: 4096,
      },
    },
    nodes: {
      grade: {
        version: 3,
        title: "Grade",
        behavior: "standard",
        style: "grade",
        parameters: {
          exposure: {
            type: "number",
            default: { kind: "number", value: 0 },
            minimum: -10,
            maximum: 10,
            step: 0.1,
            precision: 2,
          },
          mode: { type: "string", default: { kind: "string", value: "film" }, enum: ["film", "raw"] },
          file: { type: "string", default: { kind: "string", value: "" } },
          tint: { type: "color", default: { kind: "color", value: [1, 1, 1, 1] }, minimum: 0, maximum: 1 },
          ramp: {
            type: "json",
            codec: "color-ramp/v1",
            default: {
              kind: "json",
              value: {
                colorMode: "rgb",
                interpolation: "linear",
                hueInterpolation: "near",
                stops: [
                  { id: "a", position: 0, color: [0, 0, 0, 1] },
                  { id: "b", position: 1, color: [1, 1, 1, 1] },
                ],
              },
            },
          },
          lift: { type: "number", default: { kind: "number", value: 0 } },
          gamma: { type: "number", default: { kind: "number", value: 1 } },
          gain: { type: "number", default: { kind: "number", value: 1 } },
          liftColor: { type: "color", default: { kind: "color", value: [0, 0, 0, 1] } },
          gammaColor: { type: "color", default: { kind: "color", value: [0.5, 0.5, 0.5, 1] } },
          gainColor: { type: "color", default: { kind: "color", value: [1, 1, 1, 1] } },
        },
        sockets: {
          input: socket("input", "wide", { type: "number", default: { kind: "number", value: 0 } }),
          output: socket("output", "scalar"),
          secret: socket("input", "scalar"),
        },
        ui: [
          {
            kind: "text",
            variant: "header",
            title: "Color",
            visibleWhen: {
              all: [
                { parameter: "mode", equals: "film" },
                {
                  any: [
                    { parameter: "exposure", in: [0, 1] },
                    { parameter: "mode", equals: "raw" },
                  ],
                },
              ],
            },
          },
          { kind: "parameter", parameter: "exposure" },
          { kind: "parameter", parameter: "mode" },
          { kind: "resource", resource: "image", parameter: "file" },
          { kind: "parameter", parameter: "tint" },
          { kind: "widget", widget: "color-ramp", parameter: "ramp" },
          {
            kind: "widget",
            widget: "grading-wheels",
            bindings: [
              { title: "Lift", scalar: "lift", color: "liftColor" },
              { title: "Gamma", scalar: "gamma", color: "gammaColor" },
              { title: "Gain", scalar: "gain", color: "gainColor" },
            ],
          },
          { kind: "socket", socket: "input" },
          { kind: "socket", socket: "output" },
          { kind: "hidden", target: "socket", socket: "secret" },
        ],
        muteBypass: [["input", "output"]],
        migrations: [
          {
            fromVersion: 1,
            toVersion: 3,
            steps: [
              { kind: "materialize-missing", target: "parameter", key: "exposure" },
              { kind: "migrate-parameter", parameter: "ramp", codec: "color-ramp/legacy-stops" },
              { kind: "rename-socket", from: "source", to: "input" },
            ],
          },
        ],
      },
      constant: {
        version: 1,
        title: "Constant",
        behavior: "standard",
        style: "utility",
        parameters: { value: { type: "number", default: { kind: "number", value: 1 }, integer: true } },
        sockets: { out: socket("output", "scalar") },
        ui: [
          { kind: "parameter", parameter: "value" },
          { kind: "socket", socket: "out" },
        ],
        muteBypass: [],
        migrations: [],
      },
      internal: {
        version: 1,
        title: "Internal",
        behavior: "reroute",
        style: "utility",
        parameters: {},
        sockets: {},
        ui: [],
        muteBypass: [],
        migrations: [],
      },
    },
  }) as const;

const clone = () => structuredClone(valid());
function issue(value: unknown, code: string, path: string) {
  const r = validateFxNodeComposition(value);
  assert.equal(r.ok, false);
  if (!r.ok)
    assert(
      r.issues.some((x) => x.code === code && x.path === path),
      `${code} ${path}\n${JSON.stringify(r.issues)}`,
    );
}

test("valid fixture compiles without menu projection", () => {
  const v = valid();
  assert.equal(validateFxNodeComposition(v).ok, true);
  const c = compileFxNodeComposition(v);
  assert.equal("menuEntries" in c, false);
});

test("composition authoring helpers are immutable, replaceable, removable and clone-safe", () => {
  const seed = {
    schemaVersion: 2,
    id: "helpers",
    version: 1,
    compatibility: { wildcardInputTypes: [] },
    nodeStyles: { utility: { header: "#654321" } },
    resources: {},
    socketTypes: {},
    nodes: {},
  } as const satisfies FxNodeCompositionSeed;
  const themed = setTheme(seed, theme);
  assert.equal("theme" in seed, false);
  assert.notEqual(themed, seed);
  const rethemed = setTheme(themed, { ...theme, grid: "#abcdef" });
  assert.equal(themed.theme.grid, "#112233");
  assert.equal(rethemed.theme.grid, "#abcdef");
  const sockets = composeSocket(themed, "scalar", { title: "Scalar", color: "#abcdef", acceptsFrom: ["scalar"] });
  assert.deepEqual(seed.socketTypes, {});
  assert.notEqual(sockets.socketTypes, seed.socketTypes);
  const definition = {
    version: 1,
    title: "Constant",
    behavior: "standard",
    style: "utility",
    parameters: {},
    sockets: {
      out: {
        title: "Out",
        direction: "output",
        type: "scalar",
        maxIncomingLinks: 0,
        visible: true,
        value: null,
        showValue: false,
      },
    },
    ui: [{ kind: "socket", socket: "out" }],
    muteBypass: [],
    migrations: [],
  } as const;
  const nodes = composeNode(sockets, "constant", definition),
    replacement = composeNode(nodes, "constant", { ...definition, title: "Replacement" });
  assert.equal(nodes.nodes.constant.title, "Constant");
  assert.equal(replacement.nodes.constant.title, "Replacement");
  assert.deepEqual(sockets.nodes, {});
  assert.doesNotThrow(() => structuredClone(replacement));
  assert.equal(validateFxNodeComposition(replacement).ok, true);
  const withoutNode = removeNode(replacement, "constant");
  assert.equal("constant" in withoutNode.nodes, false);
  assert.equal("constant" in replacement.nodes, true);
  const dangling = removeSocket(replacement, "scalar"),
    checked = validateFxNodeComposition(dangling);
  assert.equal(checked.ok, false);
  if (!checked.ok) assert(checked.issues.some((issue) => issue.code === "reference.socketType"));
  const withSize: any = structuredClone(replacement);
  withSize.nodes.constant.defaultSize = { x: 1, y: 1 };
  issue(withSize, "shape.unknown", "/nodes/constant/defaultSize");
});

test("compilations are independent immutable clones and expose readonly facades", () => {
  const a: any = clone(),
    b: any = clone();
  a.id = "same";
  b.id = "same";
  const ca = compileFxNodeComposition(a),
    cb = compileFxNodeComposition(b);
  assert.notEqual(ca.source, cb.source);
  a.nodes.grade.parameters.exposure.default.value = 9;
  assert.equal(ca.source.nodes.grade.parameters.exposure.default.value, 0);
  assert.equal(ca.nodes.get("grade")?.typeId, "grade");
  assert(Object.isFrozen(ca));
  assert(Object.isFrozen(ca.source));
  assert(Object.isFrozen(ca.source.nodes.grade.parameters));
  for (const facade of [ca.nodes, ca.socketTypes, ca.styles, ca.resources])
    for (const method of ["set", "delete", "clear"]) assert.equal(method in facade, false);
  let leaked: unknown = "unset";
  const size = ca.nodes.size;
  ca.nodes.forEach(function () {
    leaked = arguments[2];
  });
  assert.equal(leaked, undefined);
  assert.equal(ca.nodes.size, size);
});

test("compiled image policies preserve requested source limits and expose effective ceilings", () => {
  const source: any = clone();
  source.resources.image = {
    ...source.resources.image,
    maxBytes: 100_000_000,
    maxWidth: 10_000,
    maxHeight: 3,
    maxPixels: 100_000_000,
  };
  const compiled = compileFxNodeComposition(source);
  assert.deepEqual(
    {
      maxBytes: compiled.source.resources.image.maxBytes,
      maxWidth: compiled.source.resources.image.maxWidth,
      maxHeight: compiled.source.resources.image.maxHeight,
      maxPixels: compiled.source.resources.image.maxPixels,
    },
    { maxBytes: 100_000_000, maxWidth: 10_000, maxHeight: 3, maxPixels: 100_000_000 },
  );
  const effective = compiled.resources.get("image")!;
  assert.deepEqual(
    {
      maxBytes: effective.maxBytes,
      maxWidth: effective.maxWidth,
      maxHeight: effective.maxHeight,
      maxPixels: effective.maxPixels,
    },
    { maxBytes: 33_554_432, maxWidth: 8192, maxHeight: 3, maxPixels: 24_576 },
  );
});

type Mutator = (x: any) => void;
const cases: [string, string, Mutator][] = [
  ["reference.style", "/nodes/grade/style", (x) => (x.nodes.grade.style = "no")],
  ["reference.socketType", "/socketTypes/wide/acceptsFrom/0", (x) => (x.socketTypes.wide.acceptsFrom[0] = "no")],
  ["reference.socketType", "/nodes/grade/sockets/input/type", (x) => (x.nodes.grade.sockets.input.type = "no")],
  ["reference.resource", "/nodes/grade/ui/3/resource", (x) => (x.nodes.grade.ui[3].resource = "no")],
  ["reference.parameter", "/nodes/grade/ui/1/parameter", (x) => (x.nodes.grade.ui[1].parameter = "no")],
  ["reference.socket", "/nodes/grade/ui/7/socket", (x) => (x.nodes.grade.ui[7].socket = "no")],
  [
    "reference.parameter",
    "/nodes/grade/ui/0/visibleWhen/all/0/parameter",
    (x) => (x.nodes.grade.ui[0].visibleWhen.all[0].parameter = "no"),
  ],
  [
    "schema.bounds",
    "/nodes/grade/parameters/exposure/minimum",
    (x) => (x.nodes.grade.parameters.exposure.minimum = 20),
  ],
  [
    "schema.integer",
    "/nodes/constant/parameters/value/integer",
    (x) => (x.nodes.constant.parameters.value.integer = "yes"),
  ],
  [
    "schema.precision",
    "/nodes/grade/parameters/exposure/precision",
    (x) => (x.nodes.grade.parameters.exposure.precision = 21),
  ],
  [
    "value.duplicate",
    "/nodes/grade/parameters/mode/enum",
    (x) => (x.nodes.grade.parameters.mode.enum = ["film", "film"]),
  ],
  [
    "schema.enum",
    "/nodes/grade/parameters/mode/default/value",
    (x) => (x.nodes.grade.parameters.mode.default.value = "no"),
  ],
  [
    "schema.bounds",
    "/nodes/grade/parameters/tint/default/value",
    (x) => (x.nodes.grade.parameters.tint.default.value[0] = 2),
  ],
  [
    "schema.bounds",
    "/nodes/grade/parameters/exposure/default/value",
    (x) => (x.nodes.grade.parameters.exposure.default.value = 20),
  ],
  [
    "socket.links",
    "/nodes/grade/sockets/input/maxIncomingLinks",
    (x) => (x.nodes.grade.sockets.input.maxIncomingLinks = 0),
  ],
  ["socket.output", "/nodes/grade/sockets/output", (x) => (x.nodes.grade.sockets.output.showValue = true)],
  [
    "socket.value",
    "/nodes/grade/sockets/input/value",
    (x) => {
      x.nodes.grade.sockets.input.value = null;
      x.nodes.grade.sockets.input.showValue = true;
    },
  ],
  ["ui.widget", "/nodes/grade/ui/5/widget", (x) => (x.nodes.grade.ui[5].widget = "dial")],
  ["ui.widget", "/nodes/grade/ui/5/parameter", (x) => (x.nodes.grade.ui[5].parameter = "exposure")],
  ["ui.widget", "/nodes/grade/ui/6/bindings/0/scalar", (x) => (x.nodes.grade.ui[6].bindings[0].scalar = "liftColor")],
  ["ui.widget", "/nodes/grade/ui/6/bindings/0/color", (x) => (x.nodes.grade.ui[6].bindings[0].color = "lift")],
  ["ui.resource", "/nodes/grade/ui/3/parameter", (x) => (x.nodes.grade.ui[3].parameter = "exposure")],
  ["ui.placement", "/nodes/grade/parameters/exposure", (x) => x.nodes.grade.ui.splice(1, 1)],
  ["ui.placement", "/nodes/grade/ui/2", (x) => (x.nodes.grade.ui[2] = { kind: "parameter", parameter: "exposure" })],
  ["socket.bypass", "/nodes/grade/muteBypass/0", (x) => (x.nodes.grade.muteBypass = [["output", "input"]])],
  [
    "socket.compatibility",
    "/nodes/grade/muteBypass/0",
    (x) => {
      x.nodes.grade.sockets.input.type = "scalar";
      x.nodes.grade.sockets.output.type = "wide";
    },
  ],
  ["value.duplicate", "/nodes/grade/muteBypass/1", (x) => x.nodes.grade.muteBypass.push(["input", "output"])],
  ["limit.resource", "/resources/image/maxWidth", (x) => (x.resources.image.maxWidth = 0)],
  ["shape.string", "/resources/image/referencePrefix", (x) => (x.resources.image.referencePrefix = "bad")],
  ["migration.edge", "/nodes/grade/migrations/0", (x) => (x.nodes.grade.migrations[0].toVersion = 4)],
  [
    "migration.outgoing",
    "/nodes/grade/migrations/1/fromVersion",
    (x) => x.nodes.grade.migrations.push({ fromVersion: 1, toVersion: 2, steps: [] }),
  ],
  [
    "reference.migrationTarget",
    "/nodes/grade/migrations/0/steps/0/key",
    (x) => (x.nodes.grade.migrations[0].steps[0].key = "no"),
  ],
  [
    "migration.codec",
    "/nodes/grade/migrations/0/steps/1/codec",
    (x) => (x.nodes.grade.migrations[0].steps[1].codec = "no"),
  ],
  [
    "migration.rename",
    "/nodes/grade/migrations/0/steps/3/from",
    (x) => x.nodes.grade.migrations[0].steps.push({ kind: "rename-socket", from: "source", to: "output" }),
  ],
  ["shape.literal", "/nodes/grade/behavior", (x) => (x.nodes.grade.behavior = ["standard"])],
  [
    "socket.direction",
    "/nodes/grade/sockets/input/direction",
    (x) => (x.nodes.grade.sockets.input.direction = ["input"]),
  ],
  ["ui.text", "/nodes/grade/ui/0/variant", (x) => (x.nodes.grade.ui[0].variant = ["header"])],
  ["shape.string", "/nodes/grade/ui/1/parameter", (x) => (x.nodes.grade.ui[1].parameter = 0)],
  ["shape.string", "/nodes/grade/ui/3/resource", (x) => (x.nodes.grade.ui[3].resource = 0)],
  ["ui.target", "/nodes/grade/ui/9/target", (x) => (x.nodes.grade.ui[9].target = "bogus")],
  ["ui.visibility", "/nodes/grade/ui/0/visibleWhen/all", (x) => (x.nodes.grade.ui[0].visibleWhen = { all: [] })],
  ["ui.visibility", "/nodes/grade/ui/0/visibleWhen/any", (x) => (x.nodes.grade.ui[0].visibleWhen = { any: [] })],
  [
    "migration.codec",
    "/nodes/grade/migrations/0/steps/1/parameter",
    (x) => (x.nodes.grade.migrations[0].steps[1].parameter = "exposure"),
  ],
];
test("semantic invalid cases report precise codes and paths", () => {
  for (const [code, path, mutate] of cases) {
    const x: any = clone();
    mutate(x);
    issue(x, code, path);
  }
});

test("hostile structures fail without executing code or throwing", () => {
  for (const array of [false, true]) {
    let calls = 0;
    const x: any = clone();
    if (array)
      Object.defineProperty(x.nodes.grade.ui, 0, {
        get() {
          calls++;
          return {};
        },
        enumerable: true,
      });
    else
      Object.defineProperty(x, "id", {
        get() {
          calls++;
          return "x";
        },
        enumerable: true,
      });
    issue(x, "data.inspect", array ? "/nodes/grade/ui/0" : "/id");
    assert.equal(calls, 0);
  }
  const hostile: [unknown, string, string][] = [
    [Object.assign(clone(), { [Symbol("x")]: 1 }), "data.symbol", ""],
    [Object.assign(clone(), { id: new (class X {})() }), "data.type", "/id"],
    [Object.assign(clone(), { id: new Date() }), "data.type", "/id"],
    [Object.assign(clone(), { id: new Map() }), "data.type", "/id"],
    [Object.assign(clone(), { id: new ArrayBuffer(1) }), "data.type", "/id"],
    [Object.assign(clone(), { id: 1n }), "data.type", "/id"],
    [Object.assign(clone(), { id: undefined }), "data.type", "/id"],
  ];
  for (const [x, c, p] of hostile) issue(x, c, p);
  const sparse: any = clone();
  sparse.nodes.grade.ui.length++;
  issue(sparse, "data.array", "/nodes/grade/ui/10");
  const keyed: any = [];
  Object.defineProperty(keyed, "x".repeat(FXNODE_COMPOSITION_LIMITS.maxStringCodeUnits + 1), {
    value: null,
    enumerable: true,
  });
  issue(keyed, "limit.strings", "");
  const shared: any = clone();
  shared.extra = shared.theme;
  issue(shared, "data.identity", "/extra");
  const cycle: any = clone();
  cycle.extra = cycle;
  issue(cycle, "data.identity", "/extra");
  const revoked = Proxy.revocable({}, {});
  revoked.revoke();
  for (const proxy of [
    new Proxy(
      {},
      {
        ownKeys() {
          throw Error("no");
        },
      },
    ),
    revoked.proxy,
  ])
    assert.doesNotThrow(() => issue(proxy, "data.inspect", ""));
  const np = Object.assign(Object.create(null), clone());
  const r = validateFxNodeComposition(np);
  assert.equal(r.ok, true);
  if (r.ok) assert.equal(r.value.id, "studio");
});

test("collection, visibility, and issue caps", () => {
  const limits: [string, string, (x: any) => void][] = [
    [
      "limit.collection",
      "/nodes",
      (x) => {
        const n = x.nodes.internal;
        x.nodes = {};
        for (let i = 0; i <= FXNODE_COMPOSITION_LIMITS.maxNodes; i++) x.nodes[`n${i}`] = structuredClone(n);
      },
    ],
    [
      "limit.collection",
      "/socketTypes",
      (x) => {
        for (let i = 0; i <= FXNODE_COMPOSITION_LIMITS.maxSocketTypes; i++)
          x.socketTypes[`s${i}`] = { title: "S", color: "#abcdef", acceptsFrom: [] };
      },
    ],
    [
      "limit.ui",
      "/nodes/grade/ui",
      (x) => {
        x.nodes.grade.ui = Array.from({ length: 257 }, () => ({ kind: "text", variant: "header", title: "x" }));
      },
    ],
    [
      "limit.enum",
      "/nodes/grade/parameters/mode/enum",
      (x) => (x.nodes.grade.parameters.mode.enum = Array.from({ length: 257 }, (_, i) => `v${i}`)),
    ],
    [
      "limit.migrations",
      "/nodes/grade/migrations",
      (x) =>
        (x.nodes.grade.migrations = Array.from({ length: 65 }, (_, i) => ({
          fromVersion: i + 1,
          toVersion: i + 2,
          steps: [],
        }))),
    ],
    [
      "limit.migrations",
      "/nodes/grade/migrations/0/steps",
      (x) =>
        (x.nodes.grade.migrations[0].steps = Array.from({ length: 129 }, () => ({
          kind: "materialize-missing",
          target: "parameter",
          key: "exposure",
        }))),
    ],
    [
      "limit.visibility",
      "/nodes/grade/ui/0/visibleWhen/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0/all/0",
      (x) => {
        let v: any = { parameter: "mode", equals: "film" };
        for (let i = 0; i < 17; i++) v = { all: [v] };
        x.nodes.grade.ui[0].visibleWhen = v;
      },
    ],
  ];
  for (const [c, p, m] of limits) {
    const x: any = clone();
    m(x);
    issue(x, c, p);
  }
  for (const target of ["top", "node"]) {
    const x: any = clone();
    const o = target === "top" ? x : x.nodes.grade;
    for (let i = 0; i < 110; i++) o[`unknown${i}`] = i;
    const r = validateFxNodeComposition(x);
    assert.equal(r.ok, false);
    if (!r.ok) {
      assert.equal(r.issues.length, FXNODE_COMPOSITION_LIMITS.maxIssues);
      assert(r.issues.every((i) => i.code === "shape.unknown"));
    }
  }
});

test("compile error carries frozen issues", () => {
  try {
    compileFxNodeComposition({ ...valid(), schemaVersion: 1 } as unknown as ReturnType<typeof valid>);
    assert.fail("expected error");
  } catch (e) {
    assert(e instanceof FxNodeCompositionError);
    assert(Object.isFrozen(e.issues));
    assert(Object.isFrozen(e.issues[0]));
    assert(e.issues.some((i) => i.path === "/schemaVersion"));
  }
});
