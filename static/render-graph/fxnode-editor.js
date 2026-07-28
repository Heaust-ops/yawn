import { createFxNode } from "@fxnode/index.ts";
import { CATALOG_VERSION, GRAPH_ID, fxNodeComposition } from "./catalog.js";
import { prepareBrowserHost } from "./browser-host.js";
import { createAddNodeMenu } from "./add-node-menu.js";
import { createNodeIdAllocator, spawnRequestedNode } from "./node-spawn.js";

const spec = [
  ["hdr", "texture", { x: 40, y: 170 }],
  ["depth", "texture", { x: 40, y: 300 }],
  ["mesh", "mesh", { x: 40, y: 470 }],
  ["cull", "frustum_cull", { x: 540, y: 480 }],
  ["query", "mesh_query", { x: 790, y: 330 }],
  ["registry", "pipeline_registry", { x: 790, y: 620 }],
  ["ground", "pipeline", { x: 1040, y: 290 }],
  ["pbr", "pipeline", { x: 1300, y: 290 }],
  ["pbr_double", "pipeline", { x: 1560, y: 290 }],
  ["frame_out", "frame_out", { x: 1820, y: 250 }],
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
    ["mesh", "mesh", "cull", "mesh"],
    ["mesh", "localAabbs", "cull", "localAabbs"],
    ["mesh", "mesh", "query", "mesh"],
    ["mesh", "isVisible", "query", "isVisible"],
    ["cull", "isFrustumCulled", "query", "isFrustumCulled"],
    ["mesh", "pipelineIndices", "registry", "pipelineIndices"],
    ...["ground", "pbr", "pbr_double"].flatMap((pipeline) => [
      ["mesh", "mesh", pipeline, "mesh"],
      ["query", "draws", pipeline, "draws"],
      ["registry", "activation", pipeline, "activation"],
    ]),
    ["hdr", "texture", "ground", "colorTarget"],
    ["depth", "texture", "ground", "depthTarget"],
    ["ground", "color", "pbr", "colorTarget"],
    ["ground", "depth", "pbr", "depthTarget"],
    ["pbr", "color", "pbr_double", "colorTarget"],
    ["pbr", "depth", "pbr_double", "depthTarget"],
    ["pbr_double", "color", "frame_out", "color"],
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
  depth.parameters.format = { kind: "string", value: "depth32_float" };
  for (const [id, name] of [["ground", "ground_plane"], ["pbr", "gltf_standard"], ["pbr_double", "gltf_standard_double_sided"]])
    authored.nodes.find((node) => node.id === id).parameters.pipeline = { kind: "string", value: name };
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
