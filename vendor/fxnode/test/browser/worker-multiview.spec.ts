import { expect, test } from "@playwright/test";

test("real worker supports a browser root with no attached views", async ({ page }) => {
  await page.goto("/test/browser/client-runtime.html");
  const result = await page.evaluate(async () => {
    const { createFxNode } = (await import("/src/index.ts" as string)) as typeof import("@lib/index.js");
    const { minimalStyles, numberSocket, valueNode } = (await import(
      "../../examples/minimal/definition.js"
    )) as typeof import("../../examples/minimal/definition.js");
    const root = await createFxNode({ applicationId: "worker-headless", applicationVersion: 1, resources: {} });
    await root.setHeaderStyles(minimalStyles);
    await root.composeSocket(...numberSocket);
    await root.composeNode(...valueNode);
    const added = await root.dispatch({
      type: "node.add",
      nodeId: "headless-node" as never,
      nodeType: valueNode[0],
      position: { x: 12, y: 34 },
    });
    const state = await root.getState(),
      saved = await root.getSaveData(),
      undone = await root.undo(),
      empty = await root.getState();
    root.destroy();
    return {
      addedVersion: added.version,
      undoneVersion: undone.version,
      node: state.nodes[0],
      journalLength: saved.commands.length,
      emptyCount: empty.nodes.length,
    };
  });

  expect(result.addedVersion).toBe(1);
  expect(result.undoneVersion).toBe(2);
  expect(result.node).toMatchObject({ id: "headless-node", position: { x: 12, y: 34 } });
  expect(result.journalLength).toBe(1);
  expect(result.emptyCount).toBe(0);
});

test("real worker shares graph history while views keep camera and selection", async ({ page }) => {
  await page.goto("/test/browser/client-runtime.html");
  const result = await page.evaluate(async () => {
    const { createFxNode } = (await import("/src/index.ts" as string)) as typeof import("@lib/index.js");
    const { minimalStyles, numberSocket, valueNode } = (await import(
      "../../examples/minimal/definition.js"
    )) as typeof import("../../examples/minimal/definition.js");
    const api = await createFxNode({ applicationId: "worker-multiview", applicationVersion: 1, resources: {} });
    await api.setHeaderStyles(minimalStyles);
    await api.composeSocket(...numberSocket);
    await api.composeNode(...valueNode);

    const makeCanvas = () => {
      const canvas = document.createElement("canvas"),
        context = canvas.getContext("2d")!;
      canvas.width = 400;
      canvas.height = 200;
      let frames = 0;
      const drawImage = context.drawImage.bind(context);
      context.drawImage = ((...args: Parameters<CanvasRenderingContext2D["drawImage"]>) => {
        frames++;
        Reflect.apply(drawImage, context, args);
      }) as CanvasRenderingContext2D["drawImage"];
      return { canvas, frames: () => frames };
    };
    const firstCanvas = makeCanvas(),
      secondCanvas = makeCanvas(),
      first = await api.attachView({
        canvas: firstCanvas.canvas,
        viewport: { width: 400, height: 200, dpr: 1 },
        initialCamera: { center: { x: 1_000, y: -200 }, zoom: 2 },
      }),
      second = await api.attachView({
        canvas: secondCanvas.canvas,
        viewport: { width: 400, height: 200, dpr: 1 },
      });
    await Promise.all([first.whenRendered(), second.whenRendered()]);

    const beforeRootFrames = [firstCanvas.frames(), secondCanvas.frames()];
    const rootAdd = await api.dispatch({
      type: "node.add",
      nodeId: "root-node" as never,
      nodeType: valueNode[0],
      position: { x: 0, y: 0 },
    });
    const deadline = performance.now() + 2_000;
    while (
      performance.now() < deadline &&
      (firstCanvas.frames() <= beforeRootFrames[0]! || secondCanvas.frames() <= beforeRootFrames[1]!)
    )
      await new Promise((resolve) => setTimeout(resolve, 16));
    const rootSelections = [first.getHostSnapshot().selection.nodeCount, second.getHostSnapshot().selection.nodeCount];

    const firstAdd = await first.addNode({
      typeId: valueNode[0],
      nodeId: "first-node",
      viewPosition: { x: 300, y: 50 },
    });
    const afterFirstSelection = [
      first.getHostSnapshot().selection.nodeCount,
      second.getHostSnapshot().selection.nodeCount,
    ];
    const secondAdd = await second.addNode({
      typeId: valueNode[0],
      nodeId: "second-node",
      viewPosition: { x: 200, y: 100 },
    });
    const afterSecondSelection = [
      first.getHostSnapshot().selection.nodeCount,
      second.getHostSnapshot().selection.nodeCount,
    ];
    const mute = await first.setSelectedMuted(true),
      remove = await second.removeSelected(),
      afterActions = await api.getState(),
      undo = await api.undo(),
      afterUndo = await api.getState(),
      redo = await api.redo(),
      afterRedo = await api.getState();
    const firstNode = afterActions.nodes.find((node) => node.id === "first-node");

    await first.detach();
    await second.detach();
    api.destroy();
    return {
      rootSelections,
      afterFirstSelection,
      afterSecondSelection,
      rootRenderedBoth: firstCanvas.frames() > beforeRootFrames[0]! && secondCanvas.frames() > beforeRootFrames[1]!,
      firstPosition: firstNode?.position,
      firstMuted: firstNode?.muted,
      secondRemoved: !afterActions.nodes.some((node) => node.id === "second-node"),
      undoRestoredSecond: afterUndo.nodes.some((node) => node.id === "second-node"),
      redoRemovedSecond: !afterRedo.nodes.some((node) => node.id === "second-node"),
      versions: [
        rootAdd.version,
        firstAdd.version,
        secondAdd.version,
        mute.version,
        remove.version,
        undo.version,
        redo.version,
      ],
    };
  });

  expect(result.rootSelections).toEqual([0, 0]);
  expect(result.afterFirstSelection).toEqual([1, 0]);
  expect(result.afterSecondSelection).toEqual([1, 1]);
  expect(result.rootRenderedBoth).toBe(true);
  expect(result.firstPosition).toEqual({ x: 1_050, y: -175 });
  expect(result.firstMuted).toBe(true);
  expect(result.secondRemoved).toBe(true);
  expect(result.undoRestoredSecond).toBe(true);
  expect(result.redoRemovedSecond).toBe(true);
  expect(result.versions).toEqual([1, 2, 3, 4, 5, 6, 7]);
});
