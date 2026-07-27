import { linkId, type GraphLink, type LinkId, type NodeId, type SocketId, type Vec2 } from "../core/types.js";
import type { Command } from "../commands/types.js";
import { viewToWorld } from "../layout/geometry.js";
import { GEOMETRY as G } from "../layout/constants.js";
import {
  layoutSocketsCompatible,
  type LayoutControl,
  type LayoutSnapshot,
  type Rect,
  type ViewTransform,
} from "../layout/types.js";
import { isColorRamp } from "../widgets/color-ramp.js";

const inRect = (p: Vec2, r: Rect, tolerance = 0): boolean =>
  p.x >= r.x - tolerance &&
  p.x <= r.x + r.width + tolerance &&
  p.y <= r.y + tolerance &&
  p.y >= r.y - r.height - tolerance;

export type RampTarget =
  | "add"
  | "remove"
  | "flip"
  | "distribute"
  | "mode"
  | "interpolation"
  | "hue"
  | "gradient"
  | "selector"
  | "position"
  | "swatch";
export type Hit =
  | { readonly kind: "color-wheel"; readonly id: string; readonly region: "plane" | "lightness" }
  | {
      readonly kind: "ramp";
      readonly id: string;
      readonly target: RampTarget | "handle";
      readonly stopId?: string;
      readonly position?: number;
    }
  | { readonly kind: "control-step"; readonly id: string; readonly component: number; readonly direction: -1 | 1 }
  | { readonly kind: "resource"; readonly id: string }
  | { readonly kind: "control"; readonly id: string; readonly component: number }
  | { readonly kind: "socket"; readonly id: SocketId }
  | { readonly kind: "collapse" | "resize" | "node" | "frame-header" | "frame-body"; readonly id: NodeId }
  | { readonly kind: "link"; readonly id: LinkId }
  | { readonly kind: "canvas" };
export function hitRamp(control: LayoutControl, p: Vec2, tolerance = 0): Extract<Hit, { kind: "ramp" }> | undefined {
  const b = control.rampBounds,
    v = control.value as { kind?: unknown; value?: unknown };
  if (control.kind !== "color-ramp" || !b || v.kind !== "json" || !isColorRamp(v.value)) return;
  const ramp = v.value;
  if (inRect(p, b.toolbar)) {
    const x = (p.x - b.toolbar.x) / b.toolbar.width;
    return {
      kind: "ramp",
      id: control.id,
      target: x < 0.12 ? "add" : x < 0.24 ? "remove" : x < 0.62 ? "flip" : "distribute",
    };
  }
  if (inRect(p, b.mode)) return { kind: "ramp", id: control.id, target: "mode" };
  if (inRect(p, b.interpolation)) return { kind: "ramp", id: control.id, target: "interpolation" };
  if (inRect(p, b.hue)) return { kind: "ramp", id: control.id, target: "hue" };
  if (inRect(p, b.handles, tolerance)) {
    const position = Math.max(0, Math.min(1, (p.x - b.handles.x) / b.handles.width)),
      near = ramp.stops
        .filter((s) => Math.abs(s.position - position) * b.handles.width <= Math.max(7, tolerance))
        .sort((a, c) => a.position - c.position || a.id.localeCompare(c.id));
    if (near.length) return { kind: "ramp", id: control.id, target: "handle", stopId: near[0]!.id, position };
  }
  if (inRect(p, b.gradient))
    return {
      kind: "ramp",
      id: control.id,
      target: "gradient",
      position: Math.max(0, Math.min(1, (p.x - b.gradient.x) / b.gradient.width)),
    };
  if (inRect(p, b.selector)) return { kind: "ramp", id: control.id, target: "selector" };
  if (inRect(p, b.position)) return { kind: "ramp", id: control.id, target: "position" };
  if (inRect(p, b.color)) return { kind: "ramp", id: control.id, target: "swatch" };
}
const segmentDistance = (p: Vec2, a: Vec2, b: Vec2): number => {
  const dx = b.x - a.x,
    dy = b.y - a.y,
    l = dx * dx + dy * dy;
  if (!l) return Math.hypot(p.x - a.x, p.y - a.y);
  const q = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l));
  return Math.hypot(p.x - a.x - q * dx, p.y - a.y - q * dy);
};
export function hitTest(layout: LayoutSnapshot, view: Vec2, preferredDirection?: "input" | "output"): Hit {
  const world = viewToWorld(view, layout.transform),
    tolerance = 7 / layout.transform.zoom;
  const topNode = layout.drawOrder
    .slice()
    .reverse()
    .map((id) => layout.nodes.get(id))
    .find((node) => node !== undefined && node.kind !== "frame" && inRect(world, node.bounds));
  for (const control of [...layout.controls.values()].reverse())
    if (!control.linked && (!topNode || control.nodeId === topNode.id)) {
      if (control.kind === "resource") {
        if (
          control.resourceBounds &&
          (inRect(world, control.resourceBounds.preview) || inRect(world, control.resourceBounds.open))
        )
          return { kind: "resource", id: control.id };
        continue;
      }
      if (control.colorWheelBounds) {
        if (inRect(world, control.colorWheelBounds.plane))
          return { kind: "color-wheel", id: control.id, region: "plane" };
        if (inRect(world, control.colorWheelBounds.lightness))
          return { kind: "color-wheel", id: control.id, region: "lightness" };
        continue;
      }
      const ramp = hitRamp(control, world, tolerance);
      if (ramp) return ramp;
      for (const field of control.numericFields) {
        if (inRect(world, field.decrement))
          return { kind: "control-step", id: control.id, component: field.component, direction: -1 };
        if (inRect(world, field.increment))
          return { kind: "control-step", id: control.id, component: field.component, direction: 1 };
      }
      if (inRect(world, control.bounds)) {
        const component = control.subfields.find((field) => inRect(world, field.bounds))?.index ?? 0;
        return { kind: "control", id: control.id, component };
      }
    }
  for (const id of layout.drawOrder.slice().reverse()) {
    const node = layout.nodes.get(id);
    if (node?.kind !== "reroute") continue;
    const center = { x: node.bounds.x + node.bounds.width / 2, y: node.bounds.y - node.bounds.height / 2 },
      distance = Math.hypot(world.x - center.x, world.y - center.y),
      core = Math.max(G.reroute, 7 / layout.transform.zoom),
      halo = core + 8 / layout.transform.zoom;
    if (distance > core && distance <= halo) return { kind: "node", id };
  }
  const socketHits = [...layout.sockets.values()]
    .filter(
      (socket) =>
        (!topNode || socket.nodeId === topNode.id) &&
        Math.hypot(world.x - socket.anchor.x, world.y - socket.anchor.y) <= tolerance,
    )
    .sort((a, b) => {
      const an = layout.nodes.get(a.nodeId),
        bn = layout.nodes.get(b.nodeId),
        rank = (node: typeof an) => (node ? layout.drawOrder.indexOf(node.id) : -1);
      return (
        rank(bn) - rank(an) || Number(b.direction === preferredDirection) - Number(a.direction === preferredDirection)
      );
    });
  if (socketHits[0]) return { kind: "socket", id: socketHits[0].id };
  const regular = layout.drawOrder
    .slice()
    .reverse()
    .map((id) => layout.nodes.get(id))
    .filter((n) => n?.kind !== "frame");
  for (const node of regular)
    if (node && node.kind === "node" && inRect(world, node.collapseHitRect, tolerance))
      return { kind: "collapse", id: node.id };
  for (const node of regular)
    if (node && node.kind === "node" && !node.collapsed && inRect(world, node.resizeHitRect, tolerance))
      return { kind: "resize", id: node.id };
  for (const node of regular) if (node && inRect(world, node.bounds)) return { kind: "node", id: node.id };
  for (const link of [...layout.links.values()].reverse())
    if (link.visible && link.points.slice(1).some((p, i) => segmentDistance(world, link.points[i]!, p) <= tolerance))
      return { kind: "link", id: link.id };
  for (const id of layout.drawOrder.slice().reverse()) {
    const n = layout.nodes.get(id);
    if (n?.kind === "frame" && inRect(world, n.header)) return { kind: "frame-header", id };
  }
  for (const id of layout.drawOrder.slice().reverse()) {
    const n = layout.nodes.get(id);
    if (n?.kind === "frame" && inRect(world, n.bounds)) return { kind: "frame-body", id };
  }
  return { kind: "canvas" };
}

/** Hit order is stable and all tolerances are expressed in view pixels. */
export function hitNode(layout: LayoutSnapshot, view: Vec2): NodeId | undefined {
  const hit = hitTest(layout, view);
  return hit.kind === "control" ||
    hit.kind === "control-step" ||
    hit.kind === "ramp" ||
    hit.kind === "color-wheel" ||
    hit.kind === "resource"
    ? layout.controls.get(hit.id)?.nodeId
    : hit.kind === "socket"
      ? layout.sockets.get(hit.id)?.nodeId
      : hit.kind === "node" ||
          hit.kind === "collapse" ||
          hit.kind === "resize" ||
          hit.kind === "frame-header" ||
          hit.kind === "frame-body"
        ? hit.id
        : undefined;
}

export const boxNodes = (layout: LayoutSnapshot, a: Vec2, b: Vec2): NodeId[] => {
  const p = viewToWorld(a, layout.transform),
    q = viewToWorld(b, layout.transform),
    left = Math.min(p.x, q.x),
    right = Math.max(p.x, q.x),
    top = Math.max(p.y, q.y),
    bottom = Math.min(p.y, q.y);
  return layout.drawOrder.filter((id) => {
    const n = layout.nodes.get(id);
    return (
      !!n &&
      n.kind !== "frame" &&
      n.bounds.x >= left &&
      n.bounds.x + n.bounds.width <= right &&
      n.bounds.y <= top &&
      n.bounds.y - n.bounds.height >= bottom
    );
  });
};
export const compatibleTargets = (layout: LayoutSnapshot, fromId: SocketId) => {
  const from = layout.sockets.get(fromId);
  if (!from || from.direction !== "output") return [];
  return [...layout.sockets.values()].filter((to) => to.nodeId !== from.nodeId && layoutSocketsCompatible(from, to));
};
export function planLink(
  layout: LayoutSnapshot,
  fromId: SocketId,
  toId: SocketId,
  newId: LinkId = linkId(`gesture-${fromId}-${toId}`),
): Command | undefined {
  const from = layout.sockets.get(fromId),
    to = layout.sockets.get(toId);
  if (!from || !to || !compatibleTargets(layout, fromId).some((s) => s.id === toId)) return;
  const link: GraphLink = {
    id: to.linkIds[0] ?? newId,
    fromNodeId: from.nodeId,
    fromSocketId: from.id,
    toNodeId: to.nodeId,
    toSocketId: to.id,
    muted: false,
    extensions: {},
  };
  return to.capacity === 1 && to.linkIds.length
    ? { type: "link.replace", removeId: to.linkIds[0]!, link }
    : { type: "link.add", link };
}
export const clampResize = (layout: LayoutSnapshot, id: NodeId, world: Vec2): Vec2 | undefined => {
  const n = layout.nodes.get(id);
  if (!n || n.kind !== "node") return;
  return {
    x: Math.min(G.maxWidth, Math.max(n.minimumSize.x, world.x - n.bounds.x)),
    y: Math.max(n.minimumSize.y, n.bounds.y - world.y),
  };
};
export function frameDropCandidate(layout: LayoutSnapshot, id: NodeId, world: Vec2): NodeId | undefined {
  const node = layout.nodes.get(id);
  if (!node) return;
  const descendants = new Set<NodeId>();
  for (const n of layout.nodes.values()) {
    let p = n.parentId;
    while (p) {
      if (p === id) {
        descendants.add(n.id);
        break;
      }
      p = layout.nodes.get(p)?.parentId;
    }
  }
  return [...layout.nodes.values()]
    .filter((n) => n.kind === "frame" && n.id !== id && !descendants.has(n.id) && inRect(world, n.bounds))
    .sort((a, b) => a.bounds.width * a.bounds.height - b.bounds.width * b.bounds.height)[0]?.id;
}

export function zoomAt(transform: ViewTransform, cursor: Vec2, deltaY: number): { center: Vec2; zoom: number } {
  const anchor = viewToWorld(cursor, transform);
  const zoom = Math.min(4, Math.max(0.1, transform.zoom * Math.exp(-deltaY * 0.0015)));
  return {
    zoom,
    center: {
      x: anchor.x - (cursor.x - transform.viewport.x / 2) / zoom,
      y: anchor.y + (cursor.y - transform.viewport.y / 2) / zoom,
    },
  };
}

export function groupRoots(selected: ReadonlySet<NodeId>, layout: LayoutSnapshot): readonly NodeId[] {
  return [...selected].filter((id) => {
    let parent = layout.nodes.get(id)?.parentId;
    while (parent) {
      if (selected.has(parent)) return false;
      parent = layout.nodes.get(parent)?.parentId;
    }
    return true;
  });
}
