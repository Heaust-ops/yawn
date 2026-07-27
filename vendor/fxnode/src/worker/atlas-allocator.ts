import { FXNODE_VIEW_LIMITS } from "../browser/view-limits.js";

export interface AtlasSize {
  readonly width: number;
  readonly height: number;
}
export interface AtlasRect extends AtlasSize {
  readonly x: number;
  readonly y: number;
}
export interface AtlasItem extends AtlasSize {
  readonly id: string;
}
export interface AtlasLayout extends AtlasSize {
  readonly items: ReadonlyMap<string, AtlasSize>;
  readonly regions: ReadonlyMap<string, AtlasRect>;
  readonly free: readonly AtlasRect[];
}
export type AtlasPlanKind = "allocated" | "resized-in-place" | "relocated" | "repacked" | "grown";
export type AtlasPlan =
  | {
      readonly ok: true;
      readonly kind: AtlasPlanKind;
      readonly layout: AtlasLayout;
      readonly movedIds: readonly string[];
    }
  | { readonly ok: false; readonly code: "atlas.dimension" | "atlas.capacity" };

const BLOCK = 256;
const area = ({ width, height }: AtlasSize) => width * height;
const freeOrder = (left: AtlasRect, right: AtlasRect) =>
  left.y - right.y || left.x - right.x || left.height - right.height || left.width - right.width;
const validItem = ({ width, height }: AtlasSize) =>
  Number.isSafeInteger(width) &&
  Number.isSafeInteger(height) &&
  width > 0 &&
  height > 0 &&
  width <= FXNODE_VIEW_LIMITS.maxDeviceDimension &&
  height <= FXNODE_VIEW_LIMITS.maxDeviceDimension &&
  width * height <= FXNODE_VIEW_LIMITS.maxDevicePixelsPerView;
const freezeRect = (rect: AtlasRect): AtlasRect => Object.freeze(rect);
const makeLayout = (
  width: number,
  height: number,
  items: ReadonlyMap<string, AtlasSize>,
  regions: ReadonlyMap<string, AtlasRect>,
  free: readonly AtlasRect[],
): AtlasLayout =>
  Object.freeze({
    width,
    height,
    items: new Map(items),
    regions: new Map(regions),
    free: Object.freeze(free.map(freezeRect).sort(freeOrder)),
  });

function coalesce(rectangles: readonly AtlasRect[]): AtlasRect[] {
  const result = rectangles.map((rect) => ({ ...rect }));
  for (;;) {
    let merged = false;
    outer: for (let i = 0; i < result.length; i++)
      for (let j = i + 1; j < result.length; j++) {
        const a = result[i]!,
          b = result[j]!;
        let next: AtlasRect | undefined;
        if (a.y === b.y && a.height === b.height && (a.x + a.width === b.x || b.x + b.width === a.x))
          next = { x: Math.min(a.x, b.x), y: a.y, width: a.width + b.width, height: a.height };
        else if (a.x === b.x && a.width === b.width && (a.y + a.height === b.y || b.y + b.height === a.y))
          next = { x: a.x, y: Math.min(a.y, b.y), width: a.width, height: a.height + b.height };
        if (next) {
          result.splice(j, 1);
          result.splice(i, 1, next);
          merged = true;
          break outer;
        }
      }
    if (!merged) return result.sort(freeOrder);
  }
}

function insert(layout: AtlasLayout, item: AtlasItem): AtlasLayout | undefined {
  const candidates = layout.free
    .map((rect, index) => ({ rect, index }))
    .filter(({ rect }) => item.width <= rect.width && item.height <= rect.height)
    .sort(({ rect: left }, { rect: right }) => {
      const ldw = left.width - item.width,
        ldh = left.height - item.height,
        rdw = right.width - item.width,
        rdh = right.height - item.height;
      return (
        area(left) - area(item) - (area(right) - area(item)) ||
        Math.min(ldw, ldh) - Math.min(rdw, rdh) ||
        Math.max(ldw, ldh) - Math.max(rdw, rdh) ||
        freeOrder(left, right)
      );
    });
  const selected = candidates[0];
  if (!selected) return;
  const { rect, index } = selected,
    region = { x: rect.x, y: rect.y, width: item.width, height: item.height },
    dw = rect.width - item.width,
    dh = rect.height - item.height,
    remainder: AtlasRect[] = [];
  if (dw > dh) {
    if (dw) remainder.push({ x: rect.x + item.width, y: rect.y, width: dw, height: rect.height });
    if (dh) remainder.push({ x: rect.x, y: rect.y + item.height, width: item.width, height: dh });
  } else {
    if (dh) remainder.push({ x: rect.x, y: rect.y + item.height, width: rect.width, height: dh });
    if (dw) remainder.push({ x: rect.x + item.width, y: rect.y, width: dw, height: item.height });
  }
  const items = new Map(layout.items),
    regions = new Map(layout.regions),
    free = layout.free.slice();
  items.set(item.id, Object.freeze({ width: item.width, height: item.height }));
  regions.set(item.id, freezeRect(region));
  free.splice(index, 1, ...remainder);
  return makeLayout(layout.width, layout.height, items, regions, free);
}

function emptyLayout(width: number, height: number): AtlasLayout {
  return makeLayout(width, height, new Map(), new Map(), [{ x: 0, y: 0, width, height }]);
}
function sortedItems(items: ReadonlyMap<string, AtlasSize>): AtlasItem[] {
  return [...items]
    .map(([id, size]) => ({ id, ...size }))
    .sort((left, right) => {
      const side = Math.max(right.width, right.height) - Math.max(left.width, left.height);
      return (
        side ||
        area(right) - area(left) ||
        right.height - left.height ||
        right.width - left.width ||
        (left.id < right.id ? -1 : left.id > right.id ? 1 : 0)
      );
    });
}
function pack(items: readonly AtlasItem[], width: number, height: number): AtlasLayout | undefined {
  let layout = emptyLayout(width, height);
  for (const item of items) {
    const next = insert(layout, item);
    if (!next) return;
    layout = next;
  }
  return layout;
}
function candidates(items: readonly AtlasItem[], minimumArea: number): AtlasSize[] {
  const largestWidth = Math.max(...items.map((item) => item.width)),
    largestHeight = Math.max(...items.map((item) => item.height)),
    result: AtlasSize[] = [];
  for (let width = BLOCK; width <= FXNODE_VIEW_LIMITS.maxAtlasDimension; width += BLOCK)
    for (let height = BLOCK; height <= FXNODE_VIEW_LIMITS.maxAtlasDimension; height += BLOCK) {
      const pixels = width * height;
      if (
        width >= largestWidth &&
        height >= largestHeight &&
        pixels >= minimumArea &&
        pixels <= FXNODE_VIEW_LIMITS.maxAtlasPixels
      )
        result.push({ width, height });
    }
  return result.sort(
    (left, right) =>
      area(left) - area(right) ||
      Math.abs(left.width - left.height) - Math.abs(right.width - right.height) ||
      left.width - right.width ||
      left.height - right.height,
  );
}
function repack(
  itemsMap: ReadonlyMap<string, AtlasSize>,
  current?: AtlasLayout,
  compact = false,
): AtlasLayout | undefined {
  const items = sortedItems(itemsMap),
    activeArea = items.reduce((sum, item) => sum + area(item), 0);
  if (current && !compact) {
    const same = pack(items, current.width, current.height);
    if (same) return same;
  }
  const floor =
    current && !compact
      ? Math.min(FXNODE_VIEW_LIMITS.maxAtlasPixels, Math.max(activeArea, area(current) * 2))
      : activeArea;
  for (const size of candidates(items, floor)) {
    const layout = pack(items, size.width, size.height);
    if (layout) return layout;
  }
}
function movedIds(before: AtlasLayout | undefined, after: AtlasLayout): string[] {
  return [...after.regions]
    .filter(([id, rect]) => {
      const previous = before?.regions.get(id);
      return !previous || previous.x !== rect.x || previous.y !== rect.y;
    })
    .map(([id]) => id)
    .sort();
}

export function planAtlasUpsert(current: AtlasLayout | undefined, item: AtlasItem): AtlasPlan {
  if (!validItem(item)) return { ok: false, code: "atlas.dimension" };
  const items = new Map(current?.items);
  items.set(item.id, { width: item.width, height: item.height });
  if ([...items.values()].reduce((sum, value) => sum + area(value), 0) > FXNODE_VIEW_LIMITS.maxActiveDevicePixels)
    return { ok: false, code: "atlas.capacity" };
  const previousSize = current?.items.get(item.id),
    previousRegion = current?.regions.get(item.id);
  if (
    current &&
    previousSize &&
    previousRegion &&
    item.width <= previousRegion.width &&
    item.height <= previousRegion.height
  ) {
    const nextItems = new Map(current.items);
    nextItems.set(item.id, Object.freeze({ width: item.width, height: item.height }));
    return {
      ok: true,
      kind: "resized-in-place",
      layout: makeLayout(current.width, current.height, nextItems, current.regions, current.free),
      movedIds: [],
    };
  }
  let base = current;
  if (current && previousRegion) base = removeAtlasItem(current, item.id);
  const incremental = base && insert(base, item);
  if (incremental)
    return {
      ok: true,
      kind: previousSize ? "relocated" : "allocated",
      layout: incremental,
      movedIds: movedIds(current, incremental),
    };
  const layout = repack(items, current);
  if (!layout) return { ok: false, code: "atlas.capacity" };
  return {
    ok: true,
    kind: current && layout.width === current.width && layout.height === current.height ? "repacked" : "grown",
    layout,
    movedIds: movedIds(current, layout),
  };
}

export function removeAtlasItem(current: AtlasLayout, id: string): AtlasLayout | undefined {
  const region = current.regions.get(id);
  if (!region) return current;
  const items = new Map(current.items),
    regions = new Map(current.regions);
  items.delete(id);
  regions.delete(id);
  if (!items.size) return;
  return makeLayout(current.width, current.height, items, regions, coalesce([...current.free, region]));
}

export function planAtlasCompaction(current: AtlasLayout): AtlasPlan | undefined {
  const active = [...current.items.values()].reduce((sum, item) => sum + area(item), 0);
  if (active / area(current) > 0.25) return;
  const layout = repack(current.items, current, true);
  if (!layout || area(layout) > area(current) / 2) return;
  return { ok: true, kind: "repacked", layout, movedIds: movedIds(current, layout) };
}
