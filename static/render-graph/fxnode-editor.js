import { createFxNode } from "@fxnode/index.ts";
import { CATALOG_VERSION, GRAPH_ID, fxNodeComposition } from "./catalog.js";
import { prepareBrowserHost } from "./browser-host.js";
import { createAddNodeMenu } from "./add-node-menu.js";
import { createNodeIdAllocator, spawnRequestedNode } from "./node-spawn.js";
import { culling } from "./presets.js";

async function seed(root) {
  await root.setState({
    graphId: GRAPH_ID,
    catalogVersion: CATALOG_VERSION,
    nodes: [],
    links: [],
    metadata: {},
  });
  for (const [index, item] of culling.nodes.entries())
    await root.dispatch({ type: "node.add", nodeId: item.id, nodeType: item.executor.key,
      position: { x: 40 + (index % 6) * 280, y: 120 + Math.floor(index / 6) * 260 } });
  const links = culling.nodes.flatMap((item) => Object.entries(item.inputs).map(([socket, from]) =>
    [from.node, from.socket, item.id, socket]));
  for (const [a, as, b, bs] of links) {
    const id = `${a}_${as}_${b}_${bs}`;
    await root.dispatch({
      type: "link.add",
      link: {
        id,
        fromNodeId: a,
        fromSocketId: `${a}:${as}`,
        toNodeId: b,
        toSocketId: `${b}:${bs}`,
        muted: false,
        extensions: {},
      },
    });
  }
  const authored = await root.getState();
  for (const item of culling.nodes) {
    const target = authored.nodes.find((candidate) => candidate.id === item.id);
    if (item.executor.key === "texture") {
      const texture = item.parameters.texture;
      const relative = texture.extent.kind === "surface_relative";
      const values = {
        residency: item.parameters.residency,
        format: texture.format,
        dimension: texture.dimension,
        extentMode: texture.extent.kind,
        absoluteWidth: relative ? 1 : texture.extent.width,
        absoluteHeight: relative ? 1 : texture.extent.height,
        relativeWidthNumerator: relative ? texture.extent.width.numerator : 1,
        relativeWidthDenominator: relative ? texture.extent.width.denominator : 1,
        relativeHeightNumerator: relative ? texture.extent.height.numerator : 1,
        relativeHeightDenominator: relative ? texture.extent.height.denominator : 1,
        depthOrArrayLayers: texture.extent.depthOrArrayLayers,
        mipLevelCount: texture.mipLevelCount,
        sampleCount: String(texture.sampleCount),
        viewFormat: texture.viewFormats[0] ?? "none",
      };
      for (const [key, value] of Object.entries(values))
        target.parameters[key].value = structuredClone(value);
      continue;
    }
    for (const [key, value] of Object.entries(item.parameters)) {
      const input = key.endsWith("Default") ? key.slice(0, -7) : null;
      if (input) {
        const socket = target.sockets.find((candidate) => candidate.key === input);
        if (socket?.defaultValue) socket.defaultValue.value = structuredClone(value);
      } else {
        const authoredKey = item.executor.key === "frustum_cull" && key === "camera" ? "cameraSelection" : key;
        if (target.parameters[authoredKey]) target.parameters[authoredKey].value = structuredClone(value);
      }
    }
  }
  await root.setState(authored);
}
export async function createRenderGraphEditor(canvas) {
  const allocateId = createNodeIdAllocator();
  let root,
    view,
    menu,
    destroying,
    dead = false;
  const requestAddNode = Object.assign(
    async (request, point, isCurrent = () => true) => {
      let typeId;
      try {
        typeId = await menu?.open(point);
      } catch (error) {
        if (!dead && isCurrent()) console.error(error);
        return;
      }
      if (dead || !isCurrent() || !root || !view) return;
      const alive = () => !dead && isCurrent();
      try {
        await spawnRequestedNode(
          root,
          view,
          request,
          typeId,
          allocateId,
          alive,
        );
      } catch (error) {
        if (!dead) console.error(error);
      }
    },
    { close: () => menu?.close() },
  );
  const host = prepareBrowserHost(canvas, { requestAddNode });
  const destroy = () =>
    (destroying ??= (async () => {
      dead = true;
      host.destroy();
      menu?.destroy();
      try {
        await view?.detach();
      } finally {
        root?.destroy();
        view = undefined;
        root = undefined;
      }
    })());
  try {
    root = await createFxNode({
      applicationId: "yawn.render-graph",
      applicationVersion: CATALOG_VERSION,
      resources: {},
    });
    await root.loadComposition(fxNodeComposition);
    await seed(root);
    view = await root.attachView({
      canvas,
      viewport: host.initialViewport,
      initialCamera: { center: { x: 780, y: 340 }, zoom: 0.34 },
    });
    menu = createAddNodeMenu(canvas.ownerDocument);
    host.attach(root, view);
    await view.whenRendered();
    return createEditorFacade(root, view, { requestAddNode, destroy });
  } catch (e) {
    await destroy().catch(() => {});
    throw e;
  }
}

/** Creates the small application-facing wrapper around an fxnode root and view. */
export function createEditorFacade(
  root,
  view,
  { requestAddNode = () => null, destroy = async () => {} } = {},
) {
  return {
    getState: () => root.getState(),
    getSaveData: () => root.getSaveData(),
    load: async (data) => {
      await root.load(data);
      await view.whenRendered();
    },
    loadComposition: async (data) => {
      await root.loadComposition(data);
      await view.whenRendered();
    },
    onSnapshots: (fn) =>
      root.onSnapshots((event) => fn(event.snapshot, event.version)),
    requestAddNode,
    whenRendered: () => view.whenRendered(),
    destroy,
  };
}
