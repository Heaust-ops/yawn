import { defaultPipelines } from "@yawn/default-pipelines";
import { adaptFxNodeSnapshot } from "@yawn/render-graph-fxnode";
import { CATALOG_VERSION, GRAPH_ID } from "@yawn/render-graph-fxnode/catalog";

/** Export a minimal FXNode authoring document through the shared AST boundary. */
export function fxNodeExportExample() {
  return adaptFxNodeSnapshot(
    {
      graphId: GRAPH_ID,
      catalogVersion: CATALOG_VERSION,
      nodes: [],
      links: [],
      metadata: {},
      version: 1,
    },
    1,
    { pipelines: defaultPipelines },
  );
}
