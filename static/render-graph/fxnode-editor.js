import { createFxNode } from "@fxnode/index.ts";
import { CATALOG_VERSION, GRAPH_ID, fxNodeComposition } from "./catalog.js";
import { prepareBrowserHost } from "./browser-host.js";
import { createAddNodeMenu } from "./add-node-menu.js";
import { createNodeIdAllocator, spawnRequestedNode } from "./node-spawn.js";

const spec = [
  ["surface", "surface_target", { x: 40, y: 40 }],
  ["hdr", "texture_spec", { x: 40, y: 170 }],
  ["depth", "texture_spec", { x: 40, y: 300 }],
  ["scene", "scene_table", { x: 40, y: 470 }],
  ["aabbs", "local_aabb_buffer", { x: 290, y: 430 }],
  ["frustum", "camera_frustum", { x: 290, y: 590 }],
  ["visible", "visibility_flags", { x: 290, y: 300 }],
  ["cull", "frustum_cull", { x: 540, y: 480 }],
  ["query", "mesh_query", { x: 790, y: 330 }],
  ["depth_config", "depth_stencil_config", { x: 790, y: 620 }],
  ["forward", "legacy_forward", { x: 1040, y: 290 }],
  ["copy", "fullscreen_copy", { x: 1300, y: 250 }],
  ["present", "present", { x: 1540, y: 250 }],
];
async function seed(root) {
  await root.setState({
    graphId: GRAPH_ID,
    catalogVersion: CATALOG_VERSION,
    nodes: [],
    links: [],
    metadata: {},
  });
  for (const [nodeId, nodeType, position] of spec)
    await root.dispatch({ type: "node.add", nodeId, nodeType, position });
  const links = [
    ["scene", "scene", "aabbs", "scene"],
    ["scene", "scene", "visible", "scene"],
    ["scene", "scene", "cull", "scene"],
    ["aabbs", "localAabbs", "cull", "localAabbs"],
    ["frustum", "frustum", "cull", "frustum"],
    ["scene", "scene", "query", "scene"],
    ["visible", "flags", "query", "isVisible"],
    ["cull", "flags", "query", "isFrustumCulled"],
    ["scene", "scene", "forward", "scene"],
    ["query", "draws", "forward", "draws"],
    ["hdr", "spec", "forward", "colorTarget"],
    ["depth", "spec", "forward", "depthTarget"],
    ["depth_config", "config", "forward", "depthStencil"],
    ["forward", "color", "copy", "source"],
    ["surface", "surface", "copy", "colorTarget"],
    ["copy", "color", "present", "surface"],
  ];
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
  const authored = await root.getState(),
    depth = authored.nodes.find((node) => node.id === "depth");
  depth.parameters.texture = {
    kind: "json",
    value: {
      dimension: "d2",
      format: "depth32_float",
      extent: {
        kind: "surface_relative",
        width: { numerator: 1, denominator: 1 },
        height: { numerator: 1, denominator: 1 },
        depthOrArrayLayers: 1,
      },
      mipLevelCount: 1,
      sampleCount: 1,
      viewFormats: [],
    },
  };
  await root.setState(authored);
}
export async function createRenderGraphEditor(canvas) {
  const allocateId = createNodeIdAllocator();
  let root, view, menu, destroying, dead = false;
  const requestAddNode = Object.assign(async (request, point, isCurrent = () => true) => {
    let typeId;
    try { typeId = await menu?.open(point); } catch (error) { if (!dead && isCurrent()) console.error(error); return; }
    if (dead || !isCurrent() || !root || !view) return;
    const alive = () => !dead && isCurrent();
    try { await spawnRequestedNode(root, view, request, typeId, allocateId, alive); } catch (error) { if (!dead) console.error(error); }
  }, { close: () => menu?.close() });
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
