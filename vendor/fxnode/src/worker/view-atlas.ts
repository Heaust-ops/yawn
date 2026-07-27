import {
  planAtlasCompaction,
  planAtlasUpsert,
  removeAtlasItem,
  type AtlasLayout,
  type AtlasRect,
  type AtlasSize,
} from "./atlas-allocator.js";

export type AtlasErrorCode =
  | "atlas.dimension"
  | "atlas.capacity"
  | "atlas.create"
  | "atlas.context"
  | "atlas.crop"
  | "atlas.context-lost";
export class ViewAtlasError extends Error {
  override readonly name = "ViewAtlasError";
  constructor(
    message: string,
    readonly code: AtlasErrorCode,
    readonly fatal = false,
    options?: ErrorOptions,
  ) {
    super(message, options);
  }
}
export interface ViewAtlasSlot extends AtlasRect {
  readonly viewId: string;
  readonly slotGeneration: number;
}
export interface ViewAtlasSurface {
  readonly canvas: OffscreenCanvas;
  readonly context: OffscreenCanvasRenderingContext2D;
  readonly width: number;
  readonly height: number;
  readonly atlasGeneration: number;
  readonly allocationEpoch: number;
}
export interface ViewAtlasMutation {
  readonly slot: ViewAtlasSlot;
  readonly movedViewIds: readonly string[];
  readonly invalidatedViewIds: readonly string[];
  readonly atlasGeneration: number;
  readonly allocationEpoch: number;
}
export interface ViewAtlasDetachResult {
  readonly invalidatedViewIds: readonly string[];
}
export interface ViewAtlasPaintTarget {
  readonly context: OffscreenCanvasRenderingContext2D;
  readonly deviceX: number;
  readonly deviceY: number;
  readonly deviceWidth: number;
  readonly deviceHeight: number;
}
export interface ViewAtlasPlatform {
  createCanvas(width: number, height: number): OffscreenCanvas;
  createBitmap(canvas: OffscreenCanvas, x: number, y: number, width: number, height: number): Promise<ImageBitmap>;
}
const browserPlatform: ViewAtlasPlatform = {
  createCanvas: (width, height) => new OffscreenCanvas(width, height),
  createBitmap: (canvas, x, y, width, height) => createImageBitmap(canvas, x, y, width, height),
};

export class ViewAtlasManager {
  private layout: AtlasLayout | undefined;
  private canvas: OffscreenCanvas | undefined;
  private context: OffscreenCanvasRenderingContext2D | undefined;
  private atlasGeneration = 0;
  private allocationEpoch = 0;
  private readonly slotGenerations = new Map<string, number>();
  private queue: Promise<void> = Promise.resolve();
  private disposed = false;
  private lifecycle = 0;
  private lost = false;
  private surfaceTransition = false;
  private terminalError: ViewAtlasError | undefined;
  private contextLossEpoch = 0;
  private readonly contextLost = (event: Event) => {
    if ("preventDefault" in event) event.preventDefault();
    this.contextLossEpoch++;
    this.lost = true;
  };
  private readonly contextRestored = (event: Event) => {
    const canvas = this.canvas;
    if (!canvas || event.currentTarget !== canvas) return;
    const lifecycle = this.lifecycle,
      lossEpoch = this.contextLossEpoch;
    void this.serial(async () => {
      if (this.canvas !== canvas || lossEpoch !== this.contextLossEpoch) return;
      const context = this.getContext(canvas);
      await this.probe(canvas);
      this.ensureLifecycle(lifecycle);
      if (this.canvas !== canvas || lossEpoch !== this.contextLossEpoch) return;
      this.context = context;
      this.lost = false;
      this.atlasGeneration++;
      this.allocationEpoch++;
      for (const id of this.layout?.items.keys() ?? []) this.bumpSlot(id);
    }).catch((cause) => {
      if (
        this.disposed ||
        lifecycle !== this.lifecycle ||
        this.canvas !== canvas ||
        lossEpoch !== this.contextLossEpoch
      )
        return;
      const error = new ViewAtlasError("Unable to restore the view atlas", "atlas.context-lost", true, { cause });
      this.terminalError = error;
      this.onFatal(error);
    });
  };
  constructor(
    private readonly platform: ViewAtlasPlatform = browserPlatform,
    private readonly onFatal: (error: ViewAtlasError) => void = () => {},
  ) {}

  surface(): ViewAtlasSurface | undefined {
    if (!this.canvas || !this.context || this.lost || this.surfaceTransition || this.terminalError) return;
    return Object.freeze({
      canvas: this.canvas,
      context: this.context,
      width: this.canvas.width,
      height: this.canvas.height,
      atlasGeneration: this.atlasGeneration,
      allocationEpoch: this.allocationEpoch,
    });
  }
  slot(viewId: string): ViewAtlasSlot | undefined {
    const region = this.layout?.regions.get(viewId),
      size = this.layout?.items.get(viewId),
      generation = this.slotGenerations.get(viewId);
    if (!region || !size || generation === undefined) return;
    return Object.freeze({
      viewId,
      x: region.x,
      y: region.y,
      width: size.width,
      height: size.height,
      slotGeneration: generation,
    });
  }
  attach(viewId: string, size: AtlasSize): Promise<ViewAtlasMutation> {
    return this.mutate(viewId, size, false);
  }
  resize(viewId: string, size: AtlasSize): Promise<ViewAtlasMutation> {
    return this.mutate(viewId, size, true);
  }
  renderAndCrop(
    viewId: string,
    expected: AtlasSize,
    paint: (target: ViewAtlasPaintTarget) => void,
  ): Promise<ImageBitmap | undefined> {
    return this.serial(async () => {
      const canvas = this.canvas,
        context = this.context,
        slot = this.slot(viewId),
        lifecycle = this.lifecycle,
        atlasGeneration = this.atlasGeneration,
        lossEpoch = this.contextLossEpoch;
      if (
        !canvas ||
        !context ||
        !slot ||
        this.lost ||
        this.surfaceTransition ||
        slot.width !== expected.width ||
        slot.height !== expected.height
      )
        return;
      context.save();
      try {
        context.setTransform(1, 0, 0, 1, 0, 0);
        context.beginPath();
        context.rect(slot.x, slot.y, slot.width, slot.height);
        context.clip();
        context.clearRect(slot.x, slot.y, slot.width, slot.height);
        paint({
          context,
          deviceX: slot.x,
          deviceY: slot.y,
          deviceWidth: slot.width,
          deviceHeight: slot.height,
        });
      } finally {
        context.restore();
      }
      const bitmap = await this.platform.createBitmap(canvas, slot.x, slot.y, slot.width, slot.height);
      const current = this.slot(viewId);
      if (
        this.disposed ||
        lifecycle !== this.lifecycle ||
        this.canvas !== canvas ||
        this.context !== context ||
        this.lost ||
        lossEpoch !== this.contextLossEpoch ||
        atlasGeneration !== this.atlasGeneration ||
        !current ||
        current.slotGeneration !== slot.slotGeneration ||
        bitmap.width !== expected.width ||
        bitmap.height !== expected.height
      ) {
        bitmap.close();
        return;
      }
      return bitmap;
    });
  }
  detach(viewId: string): Promise<ViewAtlasDetachResult> {
    return this.serial(async () => {
      const lifecycle = this.lifecycle;
      if (!this.layout?.items.has(viewId)) return Object.freeze({ invalidatedViewIds: Object.freeze([]) });
      const previous = this.layout,
        next = removeAtlasItem(previous, viewId);
      this.slotGenerations.delete(viewId);
      this.allocationEpoch++;
      if (!next) {
        this.unregisterContextEvents(this.canvas);
        this.layout = this.canvas = this.context = undefined;
        this.lost = false;
        this.lifecycle++;
        this.atlasGeneration++;
        return Object.freeze({ invalidatedViewIds: Object.freeze([]) });
      }
      this.layout = next;
      const compact = planAtlasCompaction(next);
      if (compact?.ok && (compact.layout.width !== next.width || compact.layout.height !== next.height))
        try {
          this.surfaceTransition = true;
          await this.resizeSurface(compact.layout.width, compact.layout.height);
          this.layout = compact.layout;
          for (const id of compact.layout.items.keys()) this.bumpSlot(id);
          this.ensureLifecycle(lifecycle);
          return Object.freeze({
            invalidatedViewIds: Object.freeze([...compact.layout.items.keys()].sort()),
          });
        } catch (error) {
          this.ensureLifecycle(lifecycle);
          this.layout = next;
          if (error instanceof ViewAtlasError && error.fatal) throw error;
          for (const id of next.items.keys()) this.bumpSlot(id);
          return Object.freeze({ invalidatedViewIds: Object.freeze([...next.items.keys()].sort()) });
        } finally {
          this.surfaceTransition = false;
        }
      this.ensureLifecycle(lifecycle);
      return Object.freeze({ invalidatedViewIds: Object.freeze([]) });
    });
  }
  dispose(): void {
    this.disposed = true;
    this.lifecycle++;
    this.unregisterContextEvents(this.canvas);
    this.layout = this.canvas = this.context = undefined;
    this.slotGenerations.clear();
    this.allocationEpoch++;
    this.atlasGeneration++;
  }
  private mutate(viewId: string, size: AtlasSize, mustExist: boolean): Promise<ViewAtlasMutation> {
    return this.serial(async () => {
      const lifecycle = this.lifecycle;
      if (mustExist && !this.layout?.items.has(viewId))
        throw new ViewAtlasError("View is not allocated in the atlas", "atlas.capacity");
      if (!mustExist && this.layout?.items.has(viewId))
        throw new ViewAtlasError("View is already allocated in the atlas", "atlas.capacity");
      const previous = this.layout,
        plan = planAtlasUpsert(previous, { id: viewId, ...size });
      if (!plan.ok)
        throw new ViewAtlasError(
          plan.code === "atlas.dimension" ? "View dimensions exceed atlas limits" : "View atlas capacity exceeded",
          plan.code,
        );
      const resized =
        !this.canvas || this.canvas.width !== plan.layout.width || this.canvas.height !== plan.layout.height;
      if (resized) this.surfaceTransition = true;
      try {
        if (resized) await this.resizeSurface(plan.layout.width, plan.layout.height);
        this.ensureLifecycle(lifecycle);
        this.layout = plan.layout;
        this.allocationEpoch++;
        const changed = new Set(plan.movedIds);
        if (
          !previous ||
          previous.items.get(viewId)?.width !== size.width ||
          previous.items.get(viewId)?.height !== size.height
        )
          changed.add(viewId);
        if (resized) for (const id of plan.layout.items.keys()) changed.add(id);
        for (const id of changed) this.bumpSlot(id);
        const slot = this.slot(viewId)!;
        return Object.freeze({
          slot,
          movedViewIds: Object.freeze(plan.movedIds.slice()),
          invalidatedViewIds: Object.freeze([...changed].sort()),
          atlasGeneration: this.atlasGeneration,
          allocationEpoch: this.allocationEpoch,
        });
      } finally {
        if (resized) this.surfaceTransition = false;
      }
    });
  }
  private bumpSlot(viewId: string): void {
    this.slotGenerations.set(viewId, (this.slotGenerations.get(viewId) ?? 0) + 1);
  }
  private async resizeSurface(width: number, height: number): Promise<void> {
    const lifecycle = this.lifecycle;
    if (!this.canvas) {
      let canvas: OffscreenCanvas;
      try {
        canvas = this.platform.createCanvas(width, height);
      } catch (cause) {
        throw new ViewAtlasError("Unable to create the view atlas", "atlas.create", false, { cause });
      }
      if (canvas.width !== width || canvas.height !== height)
        throw new ViewAtlasError("Atlas dimensions were not accepted", "atlas.create");
      this.registerContextEvents(canvas);
      const lossEpoch = this.contextLossEpoch;
      try {
        const context = this.getContext(canvas);
        await this.probe(canvas);
        this.ensureLifecycle(lifecycle);
        if (lossEpoch !== this.contextLossEpoch || this.lost)
          throw new ViewAtlasError("View atlas context was lost during creation", "atlas.context-lost");
        this.canvas = canvas;
        this.context = context;
        this.atlasGeneration++;
        return;
      } catch (error) {
        this.unregisterContextEvents(canvas);
        this.lost = false;
        throw error;
      }
    }
    const canvas = this.canvas,
      oldWidth = canvas.width,
      oldHeight = canvas.height,
      lossEpoch = this.contextLossEpoch;
    try {
      try {
        canvas.width = width;
        canvas.height = height;
      } catch (cause) {
        throw new ViewAtlasError("Unable to resize the view atlas", "atlas.create", false, { cause });
      }
      if (canvas.width !== width || canvas.height !== height)
        throw new ViewAtlasError("Atlas dimensions were not accepted", "atlas.create");
      const context = this.getContext(canvas);
      await this.probe(canvas);
      this.ensureLifecycle(lifecycle);
      if (lossEpoch !== this.contextLossEpoch || this.lost)
        throw new ViewAtlasError("View atlas context was lost while resizing", "atlas.context-lost");
      this.context = context;
      this.atlasGeneration++;
    } catch (cause) {
      if (this.disposed || lifecycle !== this.lifecycle) throw cause;
      try {
        canvas.width = oldWidth;
        canvas.height = oldHeight;
        if (canvas.width !== oldWidth || canvas.height !== oldHeight)
          throw new ViewAtlasError("Atlas dimensions could not be restored", "atlas.context-lost", true);
        this.context = this.getContext(canvas);
        await this.probe(canvas);
        this.ensureLifecycle(lifecycle);
        this.atlasGeneration++;
        // Resizing clears the canvas even when the old dimensions are restored.
        // Make every surviving slot stale so callers cannot reuse its old pixels.
        for (const id of this.layout?.items.keys() ?? []) this.bumpSlot(id);
      } catch (restoreCause) {
        this.unregisterContextEvents(canvas);
        this.canvas = this.context = undefined;
        throw new ViewAtlasError("Unable to restore the view atlas", "atlas.context-lost", true, {
          cause: restoreCause,
        });
      }
      throw cause;
    }
  }
  private getContext(canvas: OffscreenCanvas): OffscreenCanvasRenderingContext2D {
    let context: OffscreenCanvasRenderingContext2D | null;
    try {
      context = canvas.getContext("2d");
    } catch (cause) {
      throw new ViewAtlasError("Unable to create the atlas 2D context", "atlas.context", false, { cause });
    }
    if (!context) throw new ViewAtlasError("Atlas 2D context is unavailable", "atlas.context");
    return context;
  }
  private async probe(canvas: OffscreenCanvas): Promise<void> {
    let bitmap: ImageBitmap;
    try {
      bitmap = await this.platform.createBitmap(canvas, 0, 0, 1, 1);
    } catch (cause) {
      throw new ViewAtlasError("Cropped atlas bitmaps are unavailable", "atlas.crop", false, { cause });
    }
    try {
      if (bitmap.width !== 1 || bitmap.height !== 1 || typeof bitmap.close !== "function")
        throw new ViewAtlasError("Cropped atlas bitmap capability is invalid", "atlas.crop");
    } finally {
      bitmap.close?.();
    }
  }
  private ensureLifecycle(lifecycle: number): void {
    if (this.disposed || lifecycle !== this.lifecycle)
      throw new ViewAtlasError("View atlas operation was cancelled", "atlas.context-lost", true);
  }
  private registerContextEvents(canvas: OffscreenCanvas): void {
    canvas.addEventListener?.("contextlost", this.contextLost);
    canvas.addEventListener?.("contextrestored", this.contextRestored);
  }
  private unregisterContextEvents(canvas: OffscreenCanvas | undefined): void {
    canvas?.removeEventListener?.("contextlost", this.contextLost);
    canvas?.removeEventListener?.("contextrestored", this.contextRestored);
  }
  private serial<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.queue.then(() => {
      if (this.terminalError) throw this.terminalError;
      if (this.disposed) throw new ViewAtlasError("View atlas has been disposed", "atlas.context-lost", true);
      return operation();
    });
    this.queue = result.then(
      () => {},
      () => {},
    );
    return result;
  }
}
