import { createFxNode, nodeId, socketId, type GraphSnapshot } from "@lib/index.js";
import { prepareFxNodeBrowserHost } from "../shared/browser-host.js";
import { exampleTheme } from "../shared/theme.js";
import { booleanSocket, booleanValueNode, logicNodes, logicStyles } from "./definition.js";

const canvas = document.querySelector<HTMLCanvasElement>("#graph")!;
const results = document.querySelector<HTMLOutputElement>("#results")!;
const host = prepareFxNodeBrowserHost({ canvas });
let cleanedUp = false;
let unsubscribeSnapshot: (() => void) | undefined;

const operator = new Map<string, (values: readonly boolean[]) => boolean>([
  ["example.logic.and", (values) => values.every(Boolean)],
  ["example.logic.or", (values) => values.some(Boolean)],
  ["example.logic.not", (values) => !values[0]],
  ["example.logic.xor", (values) => values.filter(Boolean).length % 2 === 1],
  ["example.logic.xnor", (values) => values.filter(Boolean).length % 2 === 0],
  ["example.logic.nand", (values) => !values.every(Boolean)],
  ["example.logic.nor", (values) => !values.some(Boolean)],
]);

function evaluate(snapshot: GraphSnapshot) {
  const nodes = new Map(snapshot.nodes.map((node) => [node.id, node]));
  const resolve = (id: string, visiting = new Set<string>()): boolean => {
    const node = nodes.get(nodeId(id));
    if (!node || visiting.has(id)) return false;
    if (node.typeId === booleanValueNode[0]) {
      const value = node.parameters.value;
      return typeof value === "object" && value !== null && "kind" in value && value.kind === "boolean"
        ? value.value === true
        : false;
    }
    const operation = operator.get(node.typeId);
    if (!operation) return false;
    const next = new Set(visiting).add(id);
    const incoming = snapshot.links
      .filter((link) => link.toNodeId === node.id && link.toSocketId === socketId(`${id}:inputs`) && !link.muted)
      .sort((a, b) => a.id.localeCompare(b.id));
    return operation(incoming.map((link) => resolve(link.fromNodeId, next)));
  };
  const gates = snapshot.nodes.filter((node) => operator.has(node.typeId));
  results.replaceChildren(
    ...gates.map((node) => {
      const item = document.createElement("span");
      const value = resolve(node.id);
      item.className = value ? "true" : "false";
      item.textContent = `${node.label}: ${String(value)}`;
      return item;
    }),
  );
}

function cleanup() {
  window.removeEventListener("pagehide", cleanup);
  cleanedUp = true;
  unsubscribeSnapshot?.();
  unsubscribeSnapshot = undefined;
  const root = handle.root,
    view = handle.view;
  handle.root = null;
  handle.view = null;
  host.destroy();
  const destroyRoot = () => root?.destroy();
  if (view) void view.detach().then(destroyRoot, destroyRoot);
  else destroyRoot();
}

const handle: StandaloneExampleHandle = {
  root: null,
  view: null,
  host,
  ready: Promise.resolve(),
  cleanup,
};
window.fxnodeStandalone = handle;
window.addEventListener("pagehide", cleanup);

handle.ready = (async () => {
  try {
    const root = await createFxNode({
      applicationId: "fxnode.example.logic-nodes",
      applicationVersion: 1,
      resources: {},
    });
    if (cleanedUp) {
      root.destroy();
      return;
    }
    handle.root = root;
    await root.setTheme(exampleTheme);
    await root.setHeaderStyles(logicStyles);
    await root.composeSocket(...booleanSocket);
    await root.composeNode(...booleanValueNode);
    for (const [id, definition] of logicNodes) await root.composeNode(id, definition);
    await root.setState({ graphId: "logic-nodes", catalogVersion: 1, nodes: [], links: [], metadata: {} });

    const nodes = [
      ["a", booleanValueNode[0], { x: -460, y: 260 }],
      ["b", booleanValueNode[0], { x: -460, y: 130 }],
      ["c", booleanValueNode[0], { x: -460, y: 0 }],
      ["d", booleanValueNode[0], { x: -460, y: -130 }],
      ["e", booleanValueNode[0], { x: -460, y: -260 }],
      ["and", "example.logic.and", { x: -170, y: 220 }],
      ["or", "example.logic.or", { x: -170, y: -100 }],
      ["xor", "example.logic.xor", { x: 100, y: 180 }],
      ["not", "example.logic.not", { x: 100, y: -100 }],
      ["xnor", "example.logic.xnor", { x: 370, y: 100 }],
    ] as const;
    for (const [id, type, position] of nodes)
      await root.dispatch({ type: "node.add", nodeId: nodeId(id), nodeType: type, position });
    await root.dispatch({
      type: "node.parameter",
      id: nodeId("c"),
      key: "value",
      value: { kind: "boolean", value: false },
    });

    const connections = [
      ["a", "and"],
      ["b", "and"],
      ["c", "and"],
      ["d", "and"],
      ["e", "and"],
      ["c", "or"],
      ["d", "or"],
      ["e", "or"],
      ["and", "xor"],
      ["or", "xor"],
      ["xor", "not"],
      ["and", "xnor"],
      ["or", "xnor"],
      ["not", "xnor"],
    ] as const;
    for (const [from, to] of connections)
      await root.dispatch({
        type: "link.add",
        link: {
          fromNodeId: nodeId(from),
          fromSocketId: socketId(`${from}:${from.length === 1 ? "value" : "result"}`),
          toNodeId: nodeId(to),
          toSocketId: socketId(`${to}:inputs`),
          muted: false,
          extensions: {},
        },
      });

    const view = await root.attachView({
      canvas,
      viewport: host.initialViewport,
      initialCamera: { center: { x: 0, y: 0 }, zoom: 0.8 },
    });
    handle.view = view;
    host.attach(root, view);
    unsubscribeSnapshot = root.onSnapshots(({ snapshot }) => evaluate(snapshot));
    evaluate(await root.getState());
    await view.whenRendered();
  } catch (error) {
    cleanup();
    throw error;
  }
})();
