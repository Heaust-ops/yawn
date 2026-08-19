import { YawnCore } from "@yawn/core";
import { GltfImporter } from "@yawn/gltf-import";
import {
  CameraHandle,
  MaterialHandles,
  MeshHandles,
} from "@yawn/mesh-handles";
import { loadGraph, graphFromObject, RenderGraph, ref } from "@yawn/render-graph-js";
import {
  createGraphAst,
  reference,
  serializeGraphAst,
} from "@yawn/render-graph-ast";
import { defaultPipelines } from "@yawn/default-pipelines";
import { loadDemoLoadout } from "../render-graph-studio/demo-loadouts.js";
import { culling } from "../render-graph-studio/render-graph/presets.js";
import { installCameraRenderDataControls } from "../shared/camera-controls.js";
import { createWorkerTransport } from "../shared/create-worker-transport.js";

const waitForFrame = (core, predicate, timeout = 30_000) =>
  new Promise((resolve, reject) => {
    const current = core.telemetry;
    if (current && predicate(current)) {
      resolve(current);
      return;
    }
    const timer = setTimeout(() => {
      core.removeEventListener("renderer-frame", frame);
      reject(new Error("Renderer confirmation timed out"));
    }, timeout);
    const frame = (event) => {
      if (!predicate(event.detail)) return;
      clearTimeout(timer);
      core.removeEventListener("renderer-frame", frame);
      resolve(event.detail);
    };
    core.addEventListener("renderer-frame", frame);
  });

export function rotationY(angle) {
  const cosine = Math.cos(angle);
  const sine = Math.sin(angle);
  return [
    cosine, 0, -sine, 0,
    0, 1, 0, 0,
    sine, 0, cosine, 0,
    0, 0, 0, 1,
  ];
}

export function createPlaygroundRuntime(canvas, report) {
  let activeScene;
  const status = (message) => {
    report(String(message));
  };

  async function createScene({ loadout = "cubes", graph = culling } = {}) {
    activeScene?.dispose();
    const core = new YawnCore(createWorkerTransport(canvas));
    const handles = new MeshHandles(core);
    const importer = new GltfImporter(core);
    let stopControls = () => {};
    try {
      status("Starting render worker…");
      await core.ready;
      const compiled = await loadGraph(core, graph);
      const targetRevision = (core.telemetry?.revision ?? 0) + 1;
      const glb = await loadDemoLoadout(loadout);
      const url = URL.createObjectURL(
        new Blob([glb], { type: "model/gltf-binary" }),
      );
      let imported;
      try {
        imported = await importer.load(url);
      } finally {
        URL.revokeObjectURL(url);
      }
      const meshes = handles.fromImportedScene(imported);
      const materials = new MaterialHandles(core).fromImportedScene(imported);
      const camera = new CameraHandle(core);
      stopControls = installCameraRenderDataControls(core, canvas);
      await core.switchCompiledGraph(compiled.compiledId);
      await waitForFrame(
        core,
        (frame) =>
          frame.revision === targetRevision &&
          frame.activeCompiledGraph === graph.id &&
          frame.draws > 0 &&
          frame.gpuError === false,
      );
      activeScene = {
        core,
        handles,
        meshes,
        materials,
        camera,
        compiled: { ...compiled, graphId: graph.id },
        dispose() {
          stopControls();
          handles.dispose();
          core.dispose();
          activeScene = undefined;
        },
      };
      return activeScene;
    } catch (error) {
      stopControls();
      handles.dispose();
      core.dispose();
      throw error;
    } finally {
      importer.dispose();
    }
  }

  return Object.freeze({
    createScene,
    status,
    rotationY,
    graphs: Object.freeze({ culling }),
    packages: Object.freeze({
      createGraphAst,
      defaultPipelines,
      graphFromObject,
      reference,
      ref,
      RenderGraph,
      serializeGraphAst,
    }),
    dispose() {
      activeScene?.dispose();
    },
  });
}
