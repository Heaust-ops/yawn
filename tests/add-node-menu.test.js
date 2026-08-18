import test from "node:test";
import assert from "node:assert/strict";
import {
  addNodeItems,
  moveAddNodeSelection,
  searchAddNodeItems,
} from "../examples/render-graph-studio/render-graph/add-node-menu.js";
import {
  createNodeIdAllocator,
  spawnRequestedNode,
} from "../examples/render-graph-studio/render-graph/node-spawn.js";

test("add-node model contains all final catalog types in application groups", () => {
  assert.equal(addNodeItems.length, 44);
  assert.deepEqual(
    [...new Set(addNodeItems.map((item) => item.group))],
    ["Source", "Expression", "Render / post", "Frame"],
  );
  assert.equal(new Set(addNodeItems.map((item) => item.typeId)).size, 44);
  assert.deepEqual(addNodeItems.filter((item) => ["ground_plane", "gltf_standard", "gltf_standard_double_sided"].includes(item.typeId)).map((item) => item.title), ["Ground Plane", "glTF Standard", "glTF Standard — Double-Sided"]);
  assert.ok(!addNodeItems.some((item) => item.typeId === "pipeline" || item.title === "Pipeline"));
  assert.ok(addNodeItems.some((item) => item.typeId === "separate_u32_bits" && item.group === "Expression"));
  assert.ok(!addNodeItems.some((item) => ["mesh_query", "pipeline_registry"].includes(item.typeId)));
  assert.deepEqual(searchAddNodeItems("no such node"), []);
});

test("menu selection wraps and handles an empty search", () => {
  assert.equal(moveAddNodeSelection(0, -1, 3), 2);
  assert.equal(moveAddNodeSelection(2, 1, 3), 0);
  assert.equal(moveAddNodeSelection(0, 1, 0), -1);
});

test("allocator avoids existing and session-reserved IDs and is bounded", () => {
  const values = ["a-a", "a-a", "b-b"];
  const allocate = createNodeIdAllocator(() => values.shift());
  assert.equal(allocate(["node_aa"]), "node_bb");
  assert.throws(
    () => createNodeIdAllocator(() => "bad id")([]),
    /Unable to allocate/,
  );
});

test("all final types spawn with exact position, current version and generated ID", async () => {
  let revision = 5,
    expectedType;
  const request = { compositionRevision: 5, viewPosition: { x: 12.25, y: -4 } };
  const root = {
    getState: async () => ({ version: 91, nodes: [{ id: "existing" }] }),
  };
  const view = {
    getHostSnapshot: () => ({ compositionRevision: revision }),
    addNode: async (params, options) => {
      assert.equal(params.typeId, expectedType);
      assert.strictEqual(params.viewPosition, request.viewPosition);
      assert.match(params.nodeId, /^node_/);
      assert.deepEqual(options, { expectedVersion: 91 });
    },
  };
  let id = 0;
  const allocate = createNodeIdAllocator(
    () => `00000000-0000-0000-0000-${String(++id).padStart(12, "0")}`,
  );
  for (const item of addNodeItems) {
    expectedType = item.typeId;
    assert.equal(
      await spawnRequestedNode(root, view, request, item.typeId, allocate),
      true,
    );
  }
  revision = 6;
  assert.equal(
    await spawnRequestedNode(root, view, request, "tone_map", allocate),
    false,
  );
});

test("spawn rechecks composition after getState and propagates add errors", async () => {
  let revision = 2;
  const request = { compositionRevision: 2, viewPosition: { x: 1, y: 2 } };
  const root = {
    getState: async () => {
      revision++;
      return { version: 3, nodes: [] };
    },
  };
  const allocate = createNodeIdAllocator(() => "a");
  const view = {
    getHostSnapshot: () => ({ compositionRevision: revision }),
    addNode: async () => {
      throw Error("must not add");
    },
  };
  assert.equal(
    await spawnRequestedNode(root, view, request, "fullscreen_copy", allocate),
    false,
  );
  revision = 2;
  root.getState = async () => ({ version: 3, nodes: [] });
  await assert.rejects(
    spawnRequestedNode(root, view, request, "fullscreen_copy", allocate),
    /must not add/,
  );
});

test("spawn cancels when a pending getState becomes mutated or dead", async () => {
  const request = { compositionRevision: 2, viewPosition: { x: 1, y: 2 } };
  let resolveState,
    revision = 2,
    alive = true,
    adds = 0;
  const root = {
    getState: () =>
      new Promise((resolve) => {
        resolveState = resolve;
      }),
  };
  const view = {
    getHostSnapshot: () => ({ compositionRevision: revision }),
    addNode: async () => {
      adds++;
    },
  };
  const pendingMutation = spawnRequestedNode(
    root,
    view,
    request,
    "fullscreen_copy",
    () => "node_a",
    () => alive,
  );
  revision++;
  resolveState({ version: 1, nodes: [] });
  assert.equal(await pendingMutation, false);
  revision = 2;
  const pendingDestroy = spawnRequestedNode(
    root,
    view,
    request,
    "fullscreen_copy",
    () => "node_b",
    () => alive,
  );
  alive = false;
  resolveState({ version: 1, nodes: [] });
  assert.equal(await pendingDestroy, false);
  assert.equal(adds, 0);
});

test("spawn has a final liveness guard after ID allocation", async () => {
  let alive = true,
    adds = 0;
  const request = { compositionRevision: 1, viewPosition: { x: 0, y: 0 } };
  const root = { getState: async () => ({ version: 1, nodes: [] }) };
  const view = {
    getHostSnapshot: () => ({ compositionRevision: 1 }),
    addNode: async () => {
      adds++;
    },
  };
  const result = await spawnRequestedNode(
    root,
    view,
    request,
    "fullscreen_copy",
    () => {
      alive = false;
      return "node_reserved";
    },
    () => alive,
  );
  assert.equal(result, false);
  assert.equal(adds, 0);
});

test("spawn suppresses teardown RPC rejections but propagates genuine live add errors", async () => {
  const request = { compositionRevision: 1, viewPosition: { x: 0, y: 0 } };
  let alive = true;
  const view = {
    getHostSnapshot: () => ({ compositionRevision: 1 }),
    addNode: async () => {},
  };
  const root = {
    getState: async () => {
      alive = false;
      throw Error("detached state");
    },
  };
  assert.equal(
    await spawnRequestedNode(
      root,
      view,
      request,
      "fullscreen_copy",
      () => "node_a",
      () => alive,
    ),
    false,
  );

  alive = true;
  root.getState = async () => ({ version: 1, nodes: [] });
  view.addNode = async () => {
    alive = false;
    throw Error("detached add");
  };
  assert.equal(
    await spawnRequestedNode(
      root,
      view,
      request,
      "fullscreen_copy",
      () => "node_b",
      () => alive,
    ),
    false,
  );

  alive = true;
  view.addNode = async () => {
    throw Error("live add failure");
  };
  await assert.rejects(
    spawnRequestedNode(
      root,
      view,
      request,
      "fullscreen_copy",
      () => "node_c",
      () => alive,
    ),
    /live add failure/,
  );
});
