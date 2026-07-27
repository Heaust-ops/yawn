import type { GraphDocument, LinkId, NodeId } from "../core/types.js";
import type { CompiledFxNodeComposition, FxNodeCompositionData } from "../composition/types.js";
import { GEOMETRY as G } from "./constants.js";
import { buildLayoutScene, createLayoutView } from "./layout-graph.js";
import { LooseQuadtree } from "./spatial-index.js";
import type { LayoutScene, LayoutView, Rect, ViewTransform } from "./types.js";

const box = (r: Rect) => ({ minX: r.x, minY: r.y - r.height, maxX: r.x + r.width, maxY: r.y });
export interface LayoutStoreMetrics {
  persistentRebuilds: number;
  indexBuildMs: number;
  indexQueries: number;
  indexQueryMs: number;
}
export class IndexedLayoutStore<C extends FxNodeCompositionData> {
  scene: LayoutScene;
  readonly nodeIndex = new LooseQuadtree<NodeId>();
  readonly linkIndex = new LooseQuadtree<LinkId>();
  readonly metrics: LayoutStoreMetrics = { persistentRebuilds: 0, indexBuildMs: 0, indexQueries: 0, indexQueryMs: 0 };
  constructor(
    readonly compiled: CompiledFxNodeComposition<C>,
    document: GraphDocument<C>,
  ) {
    this.scene = buildLayoutScene(compiled, document);
    this.rebuild(document);
  }
  rebuild(document: GraphDocument<C>): void {
    const start = performance.now();
    this.scene = buildLayoutScene(this.compiled, document);
    this.nodeIndex.clear();
    this.linkIndex.clear();
    for (const [id, n] of this.scene.nodes) this.nodeIndex.insert(id, id, box(n.bounds));
    for (const [id, l] of this.scene.links) this.linkIndex.insert(id, id, box(l.bounds));
    this.metrics.persistentRebuilds++;
    this.metrics.indexBuildMs += performance.now() - start;
  }
  view(transform: ViewTransform): LayoutView {
    const start = performance.now(),
      w = transform.viewport.x / transform.zoom / 2 + G.margin,
      h = transform.viewport.y / transform.zoom / 2 + G.margin,
      q = {
        minX: transform.center.x - w,
        minY: transform.center.y - h,
        maxX: transform.center.x + w,
        maxY: transform.center.y + h,
      };
    const nodes = this.nodeIndex.query(q),
      links = this.linkIndex.query(q);
    this.metrics.indexQueries++;
    this.metrics.indexQueryMs += performance.now() - start;
    return createLayoutView(this.scene, transform, nodes, links);
  }
}
