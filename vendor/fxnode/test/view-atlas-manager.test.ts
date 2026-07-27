import assert from "node:assert/strict";
import test from "node:test";
import { ViewAtlasError, ViewAtlasManager, type ViewAtlasPlatform } from "@lib/worker/view-atlas.js";

class FakeBitmap {
  width = 1;
  height = 1;
  closed = false;
  close() {
    this.closed = true;
  }
}
class FakeCanvas extends EventTarget {
  context = {} as OffscreenCanvasRenderingContext2D;
  constructor(
    public width: number,
    public height: number,
  ) {
    super();
  }
  getContext() {
    return this.context;
  }
}
function platform(options: { failCrop?: boolean; deferred?: { resolve: (bitmap: FakeBitmap) => void } } = {}) {
  const canvases: FakeCanvas[] = [],
    bitmaps: FakeBitmap[] = [];
  const value: ViewAtlasPlatform = {
    createCanvas(width, height) {
      const canvas = new FakeCanvas(width, height);
      canvases.push(canvas);
      return canvas as unknown as OffscreenCanvas;
    },
    async createBitmap() {
      if (options.failCrop) throw new Error("crop");
      if (options.deferred)
        return new Promise((resolve) => (options.deferred!.resolve = resolve)) as Promise<ImageBitmap>;
      const bitmap = new FakeBitmap();
      bitmaps.push(bitmap);
      return bitmap as unknown as ImageBitmap;
    },
  };
  return { value, canvases, bitmaps };
}

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

test("atlas manager is lazy, serialized, and releases its only canvas after last detach", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  assert.equal(atlas.surface(), undefined);
  const [a, b] = await Promise.all([
    atlas.attach("a", { width: 100, height: 80 }),
    atlas.attach("b", { width: 90, height: 70 }),
  ]);
  assert.equal(fake.canvases.length, 1);
  assert.equal(a.slot.viewId, "a");
  assert.equal(b.slot.viewId, "b");
  assert.deepEqual(await atlas.detach("a"), { invalidatedViewIds: [] });
  assert(atlas.surface());
  assert.deepEqual(await atlas.detach("b"), { invalidatedViewIds: [] });
  assert.equal(atlas.surface(), undefined);
  await atlas.attach("c", { width: 20, height: 20 });
  assert.equal(fake.canvases.length, 2);
});

test("detach reports only survivors invalidated by successful compaction", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  await atlas.attach("large", { width: 1000, height: 1000 });
  await atlas.attach("small", { width: 100, height: 100 });
  assert.deepEqual(await atlas.detach("large"), { invalidatedViewIds: ["small"] });
  assert(atlas.slot("small"));
});

test("failed detach compaction with a successful rollback invalidates every survivor", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  await atlas.attach("large", { width: 1000, height: 1000 });
  await atlas.attach("small-a", { width: 100, height: 100 });
  await atlas.attach("small-b", { width: 90, height: 90 });
  let probes = 0;
  fake.value.createBitmap = async () => {
    if (probes++ === 0) throw new Error("compact probe failed");
    return new FakeBitmap() as unknown as ImageBitmap;
  };
  assert.deepEqual(await atlas.detach("large"), {
    invalidatedViewIds: ["small-a", "small-b"],
  });
  assert(atlas.slot("small-a"));
  assert(atlas.slot("small-b"));
});

test("failed surface transition with successful rollback stales all previous slots", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  await atlas.attach("a", { width: 100, height: 100 });
  const generation = atlas.surface()!.atlasGeneration,
    slotGeneration = atlas.slot("a")!.slotGeneration;
  let probes = 0;
  fake.value.createBitmap = async () => {
    if (probes++ === 0) throw new Error("new surface probe failed");
    return new FakeBitmap() as unknown as ImageBitmap;
  };
  await assert.rejects(atlas.attach("b", { width: 1000, height: 1000 }), (error: unknown) => {
    assert(error instanceof ViewAtlasError);
    assert.equal(error.code, "atlas.crop");
    return true;
  });
  assert.equal(atlas.slot("b"), undefined);
  assert(atlas.slot("a")!.slotGeneration > slotGeneration);
  assert(atlas.surface()!.atlasGeneration > generation);
});

test("failed first probe leaves the manager empty and retryable", async () => {
  const bad = platform({ failCrop: true }),
    atlas = new ViewAtlasManager(bad.value);
  await assert.rejects(atlas.attach("a", { width: 10, height: 10 }), (error: unknown) => {
    assert(error instanceof ViewAtlasError);
    assert.equal(error.code, "atlas.crop");
    return true;
  });
  assert.equal(atlas.surface(), undefined);
  bad.value.createBitmap = async () => new FakeBitmap() as unknown as ImageBitmap;
  await atlas.attach("a", { width: 10, height: 10 });
  assert(atlas.surface());
});

test("dispose during a pending probe cannot resurrect atlas state", async () => {
  const deferred = { resolve: (_bitmap: FakeBitmap) => {} },
    fake = platform({ deferred }),
    atlas = new ViewAtlasManager(fake.value),
    attached = atlas.attach("a", { width: 10, height: 10 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  atlas.dispose();
  const bitmap = new FakeBitmap();
  deferred.resolve(bitmap);
  await assert.rejects(attached, { name: "ViewAtlasError" });
  assert.equal(bitmap.closed, true);
  assert.equal(atlas.surface(), undefined);
});

test("context loss during initial probe cannot expose the lost candidate", async () => {
  const deferred = { resolve: (_bitmap: FakeBitmap) => {} },
    fake = platform({ deferred }),
    atlas = new ViewAtlasManager(fake.value),
    attached = atlas.attach("a", { width: 10, height: 10 });
  await tick();
  fake.canvases[0]!.dispatchEvent(new Event("contextlost", { cancelable: true }));
  deferred.resolve(new FakeBitmap());
  await assert.rejects(attached, (error: unknown) => {
    assert(error instanceof ViewAtlasError);
    assert.equal(error.code, "atlas.context-lost");
    return true;
  });
  assert.equal(atlas.surface(), undefined);
});

test("restoration events from a discarded candidate cannot affect its replacement", async () => {
  const deferred = { resolve: (_bitmap: FakeBitmap) => {} },
    fake = platform({ deferred }),
    fatals: ViewAtlasError[] = [],
    atlas = new ViewAtlasManager(fake.value, (error) => fatals.push(error)),
    first = atlas.attach("a", { width: 10, height: 10 });
  await tick();
  const discarded = fake.canvases[0]!;
  discarded.dispatchEvent(new Event("contextlost", { cancelable: true }));
  discarded.dispatchEvent(new Event("contextrestored"));
  deferred.resolve(new FakeBitmap());
  await assert.rejects(first, { name: "ViewAtlasError" });
  fake.value.createBitmap = async () => new FakeBitmap() as unknown as ImageBitmap;
  await atlas.attach("b", { width: 10, height: 10 });
  const generation = atlas.surface()!.atlasGeneration;
  discarded.dispatchEvent(new Event("contextrestored"));
  await tick();
  assert.equal(atlas.surface()!.atlasGeneration, generation);
  assert.deepEqual(fatals, []);
});

test("dispose during an existing resize does not enter rollback or republish state", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  await atlas.attach("a", { width: 10, height: 10 });
  let resolve = (_bitmap: FakeBitmap) => {};
  fake.value.createBitmap = () => new Promise((done) => (resolve = done)) as Promise<ImageBitmap>;
  const resized = atlas.resize("a", { width: 400, height: 400 });
  await tick();
  atlas.dispose();
  resolve(new FakeBitmap());
  await assert.rejects(resized, { name: "ViewAtlasError" });
  assert.equal(atlas.surface(), undefined);
});

test("context loss hides the surface and restoration reprobes before exposing it", async () => {
  const fake = platform(),
    fatals: ViewAtlasError[] = [],
    atlas = new ViewAtlasManager(fake.value, (error) => fatals.push(error));
  await atlas.attach("a", { width: 10, height: 10 });
  const before = atlas.surface()!;
  fake.canvases[0]!.dispatchEvent(new Event("contextlost", { cancelable: true }));
  assert.equal(atlas.surface(), undefined);
  fake.canvases[0]!.dispatchEvent(new Event("contextrestored"));
  for (let index = 0; index < 20 && !atlas.surface(); index++) await new Promise((resolve) => setTimeout(resolve, 0));
  const restored = atlas.surface()!;
  assert(restored);
  assert(restored.atlasGeneration > before.atlasGeneration);
  assert.deepEqual(fatals, []);
  await atlas.detach("a");
  await atlas.attach("b", { width: 10, height: 10 });
  assert(atlas.surface());
});

test("a stale restoration cannot expose or poison a newer context loss", async () => {
  const fake = platform(),
    fatals: ViewAtlasError[] = [],
    atlas = new ViewAtlasManager(fake.value, (error) => fatals.push(error));
  await atlas.attach("a", { width: 10, height: 10 });
  const canvas = fake.canvases[0]!;
  let rejectRestore = (_error: Error) => {};
  fake.value.createBitmap = () => new Promise((_resolve, reject) => (rejectRestore = reject));
  canvas.dispatchEvent(new Event("contextlost", { cancelable: true }));
  canvas.dispatchEvent(new Event("contextrestored"));
  await tick();
  canvas.dispatchEvent(new Event("contextlost", { cancelable: true }));
  rejectRestore(new Error("stale restoration"));
  await tick();
  assert.equal(atlas.surface(), undefined);
  assert.deepEqual(fatals, []);
  fake.value.createBitmap = async () => new FakeBitmap() as unknown as ImageBitmap;
  canvas.dispatchEvent(new Event("contextrestored"));
  for (let index = 0; index < 20 && !atlas.surface(); index++) await tick();
  assert(atlas.surface());
  assert.deepEqual(fatals, []);
});

test("renderAndCrop clips, clears, paints, and crops a slot without yielding", async () => {
  const calls: unknown[][] = [],
    context = new Proxy(
      {},
      {
        get:
          (_target, property) =>
          (...args: unknown[]) =>
            calls.push([property, ...args]),
      },
    ) as OffscreenCanvasRenderingContext2D,
    canvases: FakeCanvas[] = [];
  const crops: number[][] = [];
  const atlas = new ViewAtlasManager({
    createCanvas: (width, height) => {
      const canvas = new FakeCanvas(width, height);
      canvas.context = context;
      canvases.push(canvas);
      return canvas as unknown as OffscreenCanvas;
    },
    async createBitmap(_canvas, x, y, width, height) {
      crops.push([x, y, width, height]);
      return Object.assign(new FakeBitmap(), { width, height }) as unknown as ImageBitmap;
    },
  });
  await atlas.attach("a", { width: 12, height: 8 });
  calls.length = crops.length = 0;
  const bitmap = await atlas.renderAndCrop("a", { width: 12, height: 8 }, (target) => {
    calls.push(["paint", target.deviceX, target.deviceY, target.deviceWidth, target.deviceHeight]);
  });
  assert(bitmap);
  assert.deepEqual(crops, [[0, 0, 12, 8]]);
  assert.deepEqual(calls.slice(0, 6), [
    ["save"],
    ["setTransform", 1, 0, 0, 1, 0, 0],
    ["beginPath"],
    ["rect", 0, 0, 12, 8],
    ["clip"],
    ["clearRect", 0, 0, 12, 8],
  ]);
  assert.deepEqual(calls.at(-2), ["paint", 0, 0, 12, 8]);
  assert.deepEqual(calls.at(-1), ["restore"]);
});

test("detach waits for a delayed render crop and only then retires its slot", async () => {
  const fake = platform(),
    atlas = new ViewAtlasManager(fake.value);
  await atlas.attach("a", { width: 10, height: 10 });
  Object.assign(fake.canvases[0]!.context, {
    save() {},
    setTransform() {},
    beginPath() {},
    rect() {},
    clip() {},
    clearRect() {},
    restore() {},
  });
  let resolve = (_bitmap: FakeBitmap) => {},
    cropStarted = false;
  fake.value.createBitmap = () =>
    new Promise((done) => {
      cropStarted = true;
      resolve = done;
    }) as Promise<ImageBitmap>;
  const rendered = atlas.renderAndCrop("a", { width: 10, height: 10 }, () => {}),
    detached = atlas.detach("a");
  for (let index = 0; index < 10 && !cropStarted; index++) await tick();
  assert.equal(cropStarted, true);
  assert(atlas.slot("a"), "detach must wait behind the crop");
  const bitmap = Object.assign(new FakeBitmap(), { width: 10, height: 10 });
  resolve(bitmap);
  assert.equal(await rendered, bitmap, "the crop ticket remains valid until it is returned");
  assert.equal(bitmap.closed, false);
  assert.deepEqual(await detached, { invalidatedViewIds: [] });
  assert.equal(atlas.slot("a"), undefined);
});
