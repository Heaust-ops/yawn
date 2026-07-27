export interface SpatialBox {
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
}
export type SpatialRect =
  | SpatialBox
  | { readonly x: number; readonly y: number; readonly width: number; readonly height: number };

export function canonicalBox(rect: SpatialRect): SpatialBox {
  const values =
    "minX" in rect
      ? [rect.minX, rect.minY, rect.maxX, rect.maxY]
      : [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height];
  if (!values.every(Number.isFinite)) throw new TypeError("Spatial bounds must be finite");
  return {
    minX: Math.min(values[0]!, values[2]!),
    minY: Math.min(values[1]!, values[3]!),
    maxX: Math.max(values[0]!, values[2]!),
    maxY: Math.max(values[1]!, values[3]!),
  };
}
export const boxesIntersect = (a: SpatialBox, b: SpatialBox): boolean =>
  a.minX <= b.maxX && a.maxX >= b.minX && a.minY <= b.maxY && a.maxY >= b.minY;
const contains = (a: SpatialBox, b: SpatialBox): boolean =>
  a.minX <= b.minX && a.maxX >= b.maxX && a.minY <= b.minY && a.maxY >= b.maxY;

export interface SpatialIndex<T> {
  readonly size: number;
  insert(id: string, value: T, bounds: SpatialRect): void;
  update(id: string, bounds: SpatialRect, value?: T): void;
  remove(id: string): T;
  query(bounds: SpatialRect): readonly T[];
  clear(): void;
}
interface Entry<T> {
  id: string;
  value: T;
  box: SpatialBox;
}
interface Cell<T> {
  box: SpatialBox;
  entries: Entry<T>[];
  children?: [Cell<T>, Cell<T>, Cell<T>, Cell<T>];
}

/** Rectangle-capable loose quadtree. Items live in exactly one cell. */
export class LooseQuadtree<T> implements SpatialIndex<T> {
  private root: Cell<T> = { box: { minX: -1, minY: -1, maxX: 1, maxY: 1 }, entries: [] };
  private readonly locator = new Map<string, Entry<T>>();
  constructor(
    readonly maxEntries = 16,
    readonly maxDepth = 16,
    readonly minCellSize = 1,
    readonly looseness = 2,
  ) {
    if (
      !Number.isInteger(maxEntries) ||
      maxEntries < 1 ||
      !Number.isInteger(maxDepth) ||
      maxDepth < 0 ||
      !(minCellSize > 0) ||
      looseness < 1
    )
      throw new RangeError("Invalid quadtree options");
  }
  get size(): number {
    return this.locator.size;
  }
  insert(id: string, value: T, bounds: SpatialRect): void {
    if (this.locator.has(id)) throw new Error(`Duplicate spatial id: ${id}`);
    const entry = { id, value, box: canonicalBox(bounds) };
    this.locator.set(id, entry);
    this.ensureRoot(entry.box);
    this.place(this.root, entry, 0);
  }
  update(id: string, bounds: SpatialRect, value?: T): void {
    const old = this.locator.get(id);
    if (!old) throw new Error(`Unknown spatial id: ${id}`);
    const nextValue = value === undefined ? old.value : value;
    this.remove(id);
    this.insert(id, nextValue, bounds);
  }
  remove(id: string): T {
    const entry = this.locator.get(id);
    if (!entry) throw new Error(`Unknown spatial id: ${id}`);
    this.removeCell(this.root, id);
    this.locator.delete(id);
    return entry.value;
  }
  query(bounds: SpatialRect): readonly T[] {
    const query = canonicalBox(bounds),
      found: T[] = [],
      seen = new Set<string>();
    const visit = (cell: Cell<T>, root = false): void => {
      if (!boxesIntersect(root ? cell.box : this.loose(cell.box), query)) return;
      for (const e of cell.entries)
        if (!seen.has(e.id) && boxesIntersect(e.box, query)) {
          seen.add(e.id);
          found.push(e.value);
        }
      cell.children?.forEach((child) => visit(child));
    };
    visit(this.root, true);
    return found;
  }
  clear(): void {
    this.locator.clear();
    this.root = { box: { minX: -1, minY: -1, maxX: 1, maxY: 1 }, entries: [] };
  }
  private ensureRoot(box: SpatialBox): void {
    if (contains(this.root.box, box)) return;
    const all = [...this.locator.values()];
    let minX = Math.min(-1, ...all.map((e) => e.box.minX)),
      minY = Math.min(-1, ...all.map((e) => e.box.minY)),
      maxX = Math.max(1, ...all.map((e) => e.box.maxX)),
      maxY = Math.max(1, ...all.map((e) => e.box.maxY));
    let size = 2;
    while (size < Math.max(maxX - minX, maxY - minY)) size *= 2;
    const cx = (minX + maxX) / 2,
      cy = (minY + maxY) / 2;
    this.root = {
      box: { minX: cx - size / 2, minY: cy - size / 2, maxX: cx + size / 2, maxY: cy + size / 2 },
      entries: [],
    };
    for (const e of all) this.place(this.root, e, 0);
  }
  private place(cell: Cell<T>, entry: Entry<T>, depth: number): void {
    if (depth < this.maxDepth && cell.box.maxX - cell.box.minX > this.minCellSize) {
      if (!cell.children && cell.entries.length >= this.maxEntries) this.split(cell);
      const child = cell.children?.find((c) => contains(this.loose(c.box), entry.box));
      if (child) {
        this.place(child, entry, depth + 1);
        return;
      }
    }
    cell.entries.push(entry);
  }
  private split(cell: Cell<T>): void {
    const { minX, minY, maxX, maxY } = cell.box,
      mx = (minX + maxX) / 2,
      my = (minY + maxY) / 2;
    cell.children = [
      { box: { minX, minY, maxX: mx, maxY: my }, entries: [] },
      { box: { minX: mx, minY, maxX, maxY: my }, entries: [] },
      { box: { minX, minY: my, maxX: mx, maxY }, entries: [] },
      { box: { minX: mx, minY: my, maxX, maxY }, entries: [] },
    ];
  }
  private loose(box: SpatialBox): SpatialBox {
    const x = (box.minX + box.maxX) / 2,
      y = (box.minY + box.maxY) / 2,
      w = ((box.maxX - box.minX) * this.looseness) / 2,
      h = ((box.maxY - box.minY) * this.looseness) / 2;
    return { minX: x - w, minY: y - h, maxX: x + w, maxY: y + h };
  }
  private removeCell(cell: Cell<T>, id: string): boolean {
    const i = cell.entries.findIndex((e) => e.id === id);
    if (i >= 0) {
      cell.entries.splice(i, 1);
      return true;
    }
    return cell.children?.some((c) => this.removeCell(c, id)) ?? false;
  }
}
