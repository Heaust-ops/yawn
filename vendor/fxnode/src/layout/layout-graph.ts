import type { GraphDocument, GraphNode, LinkId, NodeId, SocketId, Vec2 } from "../core/types.js";
import type {
  CompiledFxNodeComposition,
  FxNodeCompositionData,
  FxNodeDefinition,
  FxNodeUiRow,
  FxNodeValueSchema,
} from "../composition/types.js";
import { GEOMETRY as G } from "./constants.js";
import { bounds, intersects } from "./geometry.js";
import type {
  LayoutControl,
  LayoutLink,
  LayoutNode,
  LayoutNumericField,
  LayoutRow,
  LayoutScene,
  LayoutSnapshot,
  LayoutSocket,
  LayoutSubfield,
  LayoutView,
  Rect,
  ViewTransform,
} from "./types.js";
import { effectivelyMutedLinks } from "./link-mute.js";
import { minimumNodeSize, nodeRowUnits, visibleNodeItems } from "./node-dimensions.js";

const title = (value: string): string => value.replace(/-/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
function controlKind(schema: FxNodeValueSchema | undefined): LayoutControl["kind"] {
  if (!schema) return "readonly-json";
  if (schema.type === "string" && schema.enum) return "enum";
  return schema.type === "json" ? "readonly-json" : schema.type;
}
function makeSubfields(bounds: Rect, type: FxNodeValueSchema["type"] | undefined): readonly LayoutSubfield[] {
  const labels = type === "vector" ? (["X", "Y", "Z"] as const) : [];
  const gutter = 3;
  const width = labels.length ? (bounds.width - gutter * (labels.length - 1)) / labels.length : 0;
  return labels.map((label, index) => ({
    index,
    label,
    bounds: { x: bounds.x + index * (width + gutter), y: bounds.y, width, height: bounds.height },
  }));
}
function makeNumericFields(
  bounds: Rect,
  schema: FxNodeValueSchema | undefined,
  subfields: readonly LayoutSubfield[],
): readonly LayoutNumericField[] {
  const fields = schema?.type === "number" ? [{ index: 0, bounds }] : schema?.type === "vector" ? subfields : [];
  const minimum =
    schema?.type === "number"
      ? (schema.softMin ?? schema.minimum)
      : schema?.type === "vector"
        ? schema.minimum
        : undefined;
  const maximum =
    schema?.type === "number"
      ? (schema.softMax ?? schema.maximum)
      : schema?.type === "vector"
        ? schema.maximum
        : undefined;
  const range =
    Number.isFinite(minimum) && Number.isFinite(maximum) && maximum! > minimum!
      ? { minimum: minimum!, maximum: maximum! }
      : undefined;
  return fields.map((field) => {
    const arrow = Math.min(7, field.bounds.width * 0.14);
    return {
      component: field.index,
      bounds: field.bounds,
      decrement: { ...field.bounds, width: arrow },
      value: { ...field.bounds, x: field.bounds.x + arrow, width: Math.max(0, field.bounds.width - arrow * 2) },
      increment: { ...field.bounds, x: field.bounds.x + field.bounds.width - arrow, width: arrow },
      ...(range ? { range } : {}),
    };
  });
}
function cubic(a: Vec2, b: Vec2): readonly Vec2[] {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  return Array.from({ length: G.linkSamples + 1 }, (_, index) => {
    const t = index / G.linkSamples,
      u = 1 - t;
    return {
      x: u ** 3 * a.x + 3 * u ** 2 * t * (a.x + dx) + 3 * u * t ** 2 * (b.x - dx) + t ** 3 * b.x,
      y: u ** 3 * a.y + 3 * u ** 2 * t * a.y + 3 * u * t ** 2 * b.y + t ** 3 * b.y,
    };
  });
}
function cubicBounds(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) {
  const points = [p0, p3];
  for (const axis of ["x", "y"] as const) {
    const a = -p0[axis] + 3 * p1[axis] - 3 * p2[axis] + p3[axis],
      b = 2 * (p0[axis] - 2 * p1[axis] + p2[axis]),
      c = p1[axis] - p0[axis],
      d = b * b - 4 * a * c;
    const roots =
      Math.abs(a) < 1e-12
        ? Math.abs(b) < 1e-12
          ? []
          : [-c / b]
        : d < 0
          ? []
          : [(-b + Math.sqrt(d)) / (2 * a), (-b - Math.sqrt(d)) / (2 * a)];
    for (const t of roots)
      if (t > 0 && t < 1) {
        const u = 1 - t;
        points.push({
          x: u ** 3 * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t ** 3 * p3.x,
          y: u ** 3 * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t ** 3 * p3.y,
        });
      }
  }
  return bounds(points);
}
export function buildLayoutScene<C extends FxNodeCompositionData>(
  compiled: CompiledFxNodeComposition<C>,
  document: GraphDocument<C>,
): LayoutScene {
  const nodes = new Map<NodeId, LayoutNode>();
  const sockets = new Map<SocketId, LayoutSocket>();
  const links = new Map<LinkId, LayoutLink>();
  const controls = new Map<string, LayoutControl>();
  const sorted = Object.values(document.nodes).sort((a, b) => a.id.localeCompare(b.id));
  const effectiveMuted = effectivelyMutedLinks(compiled, document);
  const linksBySocket = new Map<string, LinkId[]>();
  for (const link of Object.values(document.links))
    if (!effectiveMuted.has(link.id))
      for (const id of [link.fromSocketId, link.toSocketId]) {
        const list = linksBySocket.get(id) ?? [];
        list.push(link.id);
        linksBySocket.set(id, list);
      }
  const childrenByParent = new Map<string, GraphNode[]>();
  for (const node of sorted)
    if (node.parentId) {
      const list = childrenByParent.get(node.parentId) ?? [];
      list.push(node);
      childrenByParent.set(node.parentId, list);
    }
  const origins = new Map<string, Vec2>(),
    depths = new Map<string, number>();
  const resolve = (node: GraphNode): Vec2 => {
    const known = origins.get(node.id);
    if (known) return known;
    const parent = node.parentId ? document.nodes[node.parentId] : undefined,
      p = parent ? resolve(parent) : { x: 0, y: 0 };
    depths.set(node.id, parent ? depths.get(parent.id)! + 1 : 0);
    const at = { x: p.x + node.position.x, y: p.y + node.position.y };
    origins.set(node.id, at);
    return at;
  };
  for (const node of sorted) {
    const at = resolve(node),
      descriptor = node.known ? (compiled.nodes.get(node.typeId) as FxNodeDefinition | undefined) : undefined;
    if (node.known && !descriptor) throw new Error(`Missing compiled node definition: ${node.typeId}`);
    const kind = descriptor?.behavior === "frame" ? "frame" : descriptor?.behavior === "reroute" ? "reroute" : "node";
    const descriptorSockets = new Map(Object.entries(descriptor?.sockets ?? {}));
    const visibleSockets = node.sockets.filter((socket) => {
      const row = descriptor?.ui.find(
        (row) =>
          (row.kind === "socket" || (row.kind === "hidden" && row.target === "socket")) && row.socket === socket.key,
      );
      return (
        socket.visible && (!descriptor || (row?.kind === "socket" && visibleNodeItems(descriptor, node).includes(row)))
      );
    });
    const ui: readonly FxNodeUiRow[] = descriptor?.ui ?? [
      ...Object.keys(node.parameters)
        .sort()
        .map((parameter) => ({ kind: "parameter" as const, parameter })),
      ...node.sockets.map((socket) => ({ kind: "socket" as const, socket: socket.key })),
    ];
    const expandedItems = kind === "node" ? (descriptor ? visibleNodeItems(descriptor, node) : ui) : [];
    const visibleItems = node.collapsed ? [] : expandedItems;
    const contentHeight =
      kind === "frame"
        ? Math.max(G.frameMinimum, node.size.y)
        : kind === "reroute"
          ? G.reroute * 2
          : node.collapsed
            ? G.header
            : G.header + visibleItems.reduce((sum, item) => sum + nodeRowUnits(item), 0) * G.row + G.gap;
    const calculated = descriptor ? minimumNodeSize(descriptor, node) : { x: G.minWidth, y: contentHeight };
    const minimumSize = { x: calculated.x, y: kind === "node" && node.collapsed ? G.header : calculated.y };
    const width =
      kind === "reroute"
        ? G.reroute * 2
        : kind === "frame"
          ? Math.max(minimumSize.x, node.size.x)
          : Math.min(G.maxWidth, Math.max(minimumSize.x, node.size.x));
    const height = kind === "node" && !node.collapsed ? Math.max(contentHeight, node.size.y) : contentHeight;
    const nodeBounds = { x: at.x, y: at.y, width, height };
    const rowBySocket = new Map<string, number>();
    let socketRowOffset = 0;
    for (const item of visibleItems) {
      if (item.kind === "socket") rowBySocket.set(item.socket, socketRowOffset);
      socketRowOffset += nodeRowUnits(item);
    }
    const layoutSockets: LayoutSocket[] = visibleSockets.map((socket) => {
      const linkIds = linksBySocket.get(socket.id) ?? [];
      const linked = linkIds.length > 0;
      const row = rowBySocket.get(socket.key) ?? 0;
      const placement = descriptor?.ui.find((item) => item.kind === "socket" && item.socket === socket.key);
      const socketType = descriptor ? compiled.socketTypes.get(socket.dataType as never) : undefined;
      if (descriptor && !socketType) throw new Error(`Missing compiled socket type: ${socket.dataType}`);
      return {
        id: socket.id,
        nodeId: node.id,
        label: placement?.kind === "socket" ? (placement.title ?? title(socket.label)) : title(socket.label),
        dataType: socket.dataType as LayoutSocket["dataType"],
        color: socketType?.color ?? compiled.theme.unknownSocket,
        wildcardInput:
          socket.direction === "input" && compiled.compatibility.wildcardInputTypes.includes(socket.dataType),
        direction: socket.direction,
        accepts: socket.accepts,
        capacity: socket.maxIncomingLinks,
        linkIds,
        linked,
        anchor:
          kind === "reroute"
            ? { x: at.x + G.reroute, y: at.y - G.reroute }
            : {
                x: at.x + (socket.direction === "output" ? width : 0),
                y: at.y - (node.collapsed ? G.half : G.header + G.half + row * G.row),
              },
      };
    });
    for (const socket of layoutSockets) sockets.set(socket.id, socket);
    const rows: LayoutRow[] = [];
    let rowOffset = 0;
    for (const item of visibleItems) {
      const units = nodeRowUnits(item);
      const rowBounds: Rect = { x: at.x, y: at.y - G.header - rowOffset * G.row, width, height: units * G.row };
      if (item.kind === "text") {
        rows.push({ kind: item.variant, label: item.title, units, bounds: rowBounds });
        rowOffset += units;
        continue;
      }
      if (
        item.kind === "parameter" ||
        item.kind === "resource" ||
        (item.kind === "widget" && item.widget === "color-ramp")
      ) {
        const key = item.parameter,
          schema = descriptor?.parameters[key],
          ramp = item.kind === "widget";
        const id = `${node.id}:parameter:${key}`;
        const controlBounds =
          ramp || schema?.type === "number"
            ? { x: at.x + 10, y: rowBounds.y - 3, width: width - 20, height: ramp ? units * G.row - 6 : G.row - 6 }
            : { x: at.x + width * 0.42, y: rowBounds.y - 3, width: width * 0.53, height: G.row - 6 };
        const subfields = makeSubfields(controlBounds, schema?.type);
        const rampBounds = ramp
          ? {
              toolbar: { x: controlBounds.x, y: controlBounds.y, width: controlBounds.width, height: 20 },
              mode: { x: controlBounds.x, y: controlBounds.y - 22, width: controlBounds.width * 0.3, height: 20 },
              interpolation: {
                x: controlBounds.x + controlBounds.width * 0.31,
                y: controlBounds.y - 22,
                width: controlBounds.width * 0.4,
                height: 20,
              },
              hue: {
                x: controlBounds.x + controlBounds.width * 0.72,
                y: controlBounds.y - 22,
                width: controlBounds.width * 0.28,
                height: 20,
              },
              gradient: {
                x: controlBounds.x + 8,
                y: controlBounds.y - 46,
                width: controlBounds.width - 16,
                height: 28,
              },
              handles: { x: controlBounds.x + 8, y: controlBounds.y - 74, width: controlBounds.width - 16, height: 28 },
              selector: { x: controlBounds.x, y: controlBounds.y - 104, width: controlBounds.width * 0.25, height: 20 },
              position: {
                x: controlBounds.x + controlBounds.width * 0.27,
                y: controlBounds.y - 104,
                width: controlBounds.width * 0.35,
                height: 20,
              },
              color: { x: controlBounds.x, y: controlBounds.y - 126, width: controlBounds.width, height: 20 },
            }
          : undefined;
        const resourceBounds =
          item.kind === "resource"
            ? {
                preview: { x: at.x + 10, y: rowBounds.y - 4, width: width - 20, height: units * G.row - 34 },
                open: { x: at.x + 10, y: rowBounds.y - units * G.row + 26, width: width - 20, height: 22 },
              }
            : undefined;
        const resource = item.kind === "resource" ? compiled.resources.get(item.resource as never) : undefined;
        if (item.kind === "resource" && !resource) throw new Error(`Missing compiled resource: ${item.resource}`);
        const control: LayoutControl = {
          id,
          nodeId: node.id,
          source: descriptor ? "parameter" : "unknown",
          key,
          label: ("title" in item ? item.title : undefined) ?? resource?.title ?? title(key),
          kind: item.kind === "resource" ? "resource" : ramp ? "color-ramp" : controlKind(schema),
          value: node.parameters[key],
          ...(schema ? { schema } : {}),
          ...(resource && item.kind === "resource"
            ? {
                resourceId: item.resource,
                resourceReferencePrefix: resource.referencePrefix,
                openTitle: item.openTitle ?? resource.openTitle,
              }
            : {}),
          linked: false,
          bounds: item.kind === "resource" ? resourceBounds!.open : controlBounds,
          subfields,
          numericFields: makeNumericFields(controlBounds, schema, subfields),
          ...(rampBounds ? { rampBounds } : {}),
          ...(resourceBounds ? { resourceBounds } : {}),
        };
        controls.set(id, control);
        rows.push({ kind: "control", controlId: id, units, bounds: rowBounds });
      } else if (item.kind === "widget" && item.widget === "grading-wheels") {
        const gap = 10,
          padding = 10,
          columnWidth = (width - padding * 2 - gap * 2) / 3;
        const wheels = item.bindings.map((wheel, index) => {
          const x = at.x + padding + index * (columnWidth + gap),
            scalarSchema = descriptor?.parameters[wheel.scalar],
            colorSchema = descriptor?.parameters[wheel.color],
            scalarId = `${node.id}:parameter:${wheel.scalar}`,
            colorId = `${node.id}:parameter:${wheel.color}`,
            labelBounds = { x, y: rowBounds.y - 4, width: columnWidth, height: 18 },
            plane = { x: x + 2, y: rowBounds.y - 25, width: columnWidth - 24, height: columnWidth - 24 },
            lightness = { x: x + columnWidth - 18, y: rowBounds.y - 25, width: 14, height: columnWidth - 24 },
            scalarBounds = {
              x: x + 2,
              y: rowBounds.y - 25 - (columnWidth - 24) - 10,
              width: columnWidth - 6,
              height: 18,
            },
            colorBounds = {
              x: plane.x,
              y: plane.y,
              width: lightness.x + lightness.width - plane.x,
              height: plane.height,
            };
          controls.set(scalarId, {
            id: scalarId,
            nodeId: node.id,
            source: "parameter",
            key: wheel.scalar,
            label: wheel.title,
            kind: controlKind(scalarSchema),
            value: node.parameters[wheel.scalar],
            ...(scalarSchema ? { schema: scalarSchema } : {}),
            linked: false,
            bounds: scalarBounds,
            subfields: [],
            numericFields: makeNumericFields(scalarBounds, scalarSchema, []),
          });
          controls.set(colorId, {
            id: colorId,
            nodeId: node.id,
            source: "parameter",
            key: wheel.color,
            label: wheel.title,
            kind: controlKind(colorSchema),
            value: node.parameters[wheel.color],
            ...(colorSchema ? { schema: colorSchema } : {}),
            linked: false,
            bounds: colorBounds,
            subfields: [],
            numericFields: [],
            colorWheelBounds: { plane, lightness },
          });
          return { label: wheel.title, labelBounds, scalarControlId: scalarId, colorControlId: colorId };
        });
        rows.push({
          kind: "grading-wheels",
          wheels: wheels as unknown as Extract<LayoutRow, { kind: "grading-wheels" }>["wheels"],
          units,
          bounds: rowBounds,
        });
      } else if (item.kind === "socket") {
        const raw = node.sockets.find((socket) => socket.key === item.socket);
        const socket = raw && sockets.get(raw.id);
        if (!raw || !socket) continue;
        const socketDescriptor = descriptorSockets.get(item.socket),
          schema = socketDescriptor?.showValue ? (socketDescriptor.value ?? undefined) : undefined;
        let controlId: string | undefined;
        if (schema && socket.direction === "input") {
          controlId = `${node.id}:socket:${socket.id}`;
          const controlBounds =
            schema.type === "number"
              ? { x: at.x + 12, y: rowBounds.y - 3, width: width - 24, height: G.row - 6 }
              : { x: at.x + width * 0.42, y: rowBounds.y - 3, width: width * 0.53, height: G.row - 6 };
          const subfields = makeSubfields(controlBounds, schema.type);
          controls.set(controlId, {
            id: controlId,
            nodeId: node.id,
            source: "socket-default",
            key: socket.id,
            label: item.title ?? socket.label,
            kind: controlKind(schema),
            value: raw.defaultValue,
            schema,
            linked: socket.linked,
            bounds: controlBounds,
            subfields,
            numericFields: makeNumericFields(controlBounds, schema, subfields),
          });
        }
        rows.push({
          kind: "socket",
          socketId: socket.id,
          ...(controlId ? { controlId } : {}),
          units: 1,
          bounds: rowBounds,
        });
      }
      rowOffset += units;
    }
    if (kind === "reroute" && layoutSockets[0]) {
      rows.push({ kind: "socket", socketId: layoutSockets[0].id, units: 1, bounds: nodeBounds });
    }
    const byKey = new Map(node.sockets.map((s) => [s.key, s.id]));
    const bypasses = node.muted
      ? (descriptor?.muteBypass ?? []).flatMap(([a, b]) => {
          const from = byKey.get(a),
            to = byKey.get(b),
            aa = from && sockets.get(from)?.anchor,
            bb = to && sockets.get(to)?.anchor;
          return aa && bb ? [{ from: aa, to: bb }] : [];
        })
      : [];
    const style = descriptor && compiled.styles.get(descriptor.style as never);
    if (descriptor && !style) throw new Error(`Missing compiled style: ${descriptor.style}`);
    nodes.set(node.id, {
      id: node.id,
      ...(node.parentId ? { parentId: node.parentId } : {}),
      typeId: node.typeId,
      label: node.label,
      ...(descriptor ? { styleId: descriptor.style } : {}),
      headerColor: style?.header ?? compiled.theme.unknownHeader,
      kind,
      localPosition: node.position,
      worldPosition: at,
      authoredSize: node.size,
      minimumSize,
      bounds: nodeBounds,
      header: { x: at.x, y: at.y, width, height: kind === "reroute" ? 0 : G.header },
      collapseHitRect: { x: at.x, y: at.y, width: 14, height: G.header },
      resizeHitRect: {
        x: at.x + width - G.resize,
        y: at.y - height + G.resize,
        width: G.resize * 2,
        height: G.resize * 2,
      },
      collapsed: node.collapsed,
      muted: node.muted,
      visible: true,
      rows,
      bypasses,
    } satisfies LayoutNode);
  }
  // Frames are behind all regular nodes. Their authored size is expanded to contain direct children.
  for (const frame of sorted
    .filter((node) => nodes.get(node.id)?.kind === "frame")
    .sort((a, b) => depths.get(b.id)! - depths.get(a.id)!)) {
    const children = (childrenByParent.get(frame.id) ?? []).map((node) => nodes.get(node.id) as LayoutNode);
    if (children.length) {
      const childBounds = bounds(
        children.flatMap((child) => [
          { x: child.bounds.x, y: child.bounds.y },
          { x: child.bounds.x + child.bounds.width, y: child.bounds.y - child.bounds.height },
        ]),
      );
      const current = nodes.get(frame.id) as LayoutNode;
      const fitted = {
        x: Math.min(current.bounds.x, childBounds.x - G.frameMargin),
        y: Math.max(current.bounds.y, childBounds.y + G.frameMargin),
        width:
          Math.max(current.bounds.x + current.bounds.width, childBounds.x + childBounds.width + G.frameMargin) -
          Math.min(current.bounds.x, childBounds.x - G.frameMargin),
        height:
          Math.max(current.bounds.y, childBounds.y + G.frameMargin) -
          Math.min(current.bounds.y - current.bounds.height, childBounds.y - childBounds.height - G.frameMargin),
      };
      nodes.set(frame.id, { ...current, bounds: fitted, header: { ...fitted, height: G.header } });
    }
  }
  for (const link of Object.values(document.links).sort((a, b) => a.id.localeCompare(b.id))) {
    const from = sockets.get(link.fromSocketId) as LayoutSocket | undefined,
      to = sockets.get(link.toSocketId) as LayoutSocket | undefined;
    if (!from || !to) continue;
    const points = cubic(from.anchor, to.anchor);
    const dx = Math.max(40, Math.abs(to.anchor.x - from.anchor.x) * 0.5);
    const cs = [
        { x: from.anchor.x + dx, y: from.anchor.y },
        { x: to.anchor.x - dx, y: to.anchor.y },
      ] as const,
      linkBounds = cubicBounds(from.anchor, cs[0], cs[1], to.anchor);
    links.set(link.id, {
      id: link.id,
      fromNodeId: link.fromNodeId,
      fromSocketId: link.fromSocketId,
      toNodeId: link.toNodeId,
      toSocketId: link.toSocketId,
      dataType: from.dataType,
      color: from.color,
      points,
      controls: cs,
      bounds: linkBounds,
      visible: true,
      muted: effectiveMuted.has(link.id),
    } satisfies LayoutLink);
  }
  const allBounds = [...nodes.values()].map((node: LayoutNode) => node.bounds);
  const graphBounds = allBounds.length
    ? bounds(
        allBounds.flatMap((rect) => [
          { x: rect.x, y: rect.y },
          { x: rect.x + rect.width, y: rect.y - rect.height },
        ]),
      )
    : { x: 0, y: 0, width: 0, height: 0 };
  const drawOrder = sorted
    .filter((node) => nodes.get(node.id)?.kind === "frame")
    .concat(sorted.filter((node) => nodes.get(node.id)?.kind !== "frame"))
    .map((node) => node.id);
  return {
    nodes,
    sockets,
    controls,
    links,
    drawOrder,
    graphBounds,
    nodeRanks: new Map(drawOrder.map((id, i) => [id, i])),
    linkRanks: new Map([...links.keys()].map((id, i) => [id, i])),
  };
}
export function createLayoutView(
  scene: LayoutScene,
  transform: ViewTransform,
  nodeIds: readonly NodeId[] = scene.drawOrder,
  linkIds: readonly LinkId[] = [...scene.links.keys()],
): LayoutView {
  const viewport = {
    x: transform.center.x - transform.viewport.x / transform.zoom / 2,
    y: transform.center.y + transform.viewport.y / transform.zoom / 2,
    width: transform.viewport.x / transform.zoom,
    height: transform.viewport.y / transform.zoom,
  };
  const ns = nodeIds
      .filter((id) => {
        const n = scene.nodes.get(id);
        return n && intersects(n.bounds, viewport, G.margin);
      })
      .sort((a, b) => (scene.nodeRanks.get(a) ?? 0) - (scene.nodeRanks.get(b) ?? 0)),
    ls = linkIds
      .filter((id) => {
        const l = scene.links.get(id);
        return l && intersects(l.bounds, viewport, G.margin);
      })
      .sort((a, b) => (scene.linkRanks.get(a) ?? 0) - (scene.linkRanks.get(b) ?? 0));
  return {
    ...scene,
    drawOrder: ns,
    transform,
    candidateNodeIds: ns,
    candidateLinkIds: ls,
    totalNodes: scene.nodes.size,
    totalLinks: scene.links.size,
  };
}
export function layoutGraph<C extends FxNodeCompositionData>(
  compiled: CompiledFxNodeComposition<C>,
  document: GraphDocument<C>,
  transform: ViewTransform,
): LayoutSnapshot {
  return createLayoutView(buildLayoutScene(compiled, document), transform);
}
export function applyNodeOrder<T extends LayoutSnapshot>(layout: T, order: readonly NodeId[]): T {
  const frames = layout.drawOrder.filter((id) => layout.nodes.get(id)?.kind === "frame"),
    ordinary = layout.drawOrder.filter((id) => layout.nodes.get(id)?.kind !== "frame"),
    available = new Set(ordinary),
    promoted = order.filter((id) => available.has(id)),
    raised = new Set(promoted);
  return { ...layout, drawOrder: [...frames, ...ordinary.filter((id) => !raised.has(id)), ...promoted] };
}
