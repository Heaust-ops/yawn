import { GEOMETRY as G } from "../layout/constants.js";
import { worldToView } from "../layout/geometry.js";
import type { LinkId, NodeId, ParameterValue, SocketId } from "../core/types.js";
import {
  layoutSocketsCompatible,
  type LayoutControl,
  type LayoutSnapshot,
  type LayoutSocket,
  type Rect,
  type ViewTransform,
} from "../layout/types.js";
import type { FxNodeTheme } from "../composition/types.js";
import { isColorRamp, sampleColorRamp } from "../widgets/color-ramp.js";
import { oklabToOklch, srgbToOklab } from "../color/oklab.js";
import { paintOklchWheel, type DevicePaintTarget } from "./color-picker-renderer.js";

export interface InteractionRenderState {
  readonly knife?: {
    points: readonly { x: number; y: number }[];
    crossed: ReadonlySet<LinkId>;
    mode: "remove" | "mute";
  };
}
export interface InteractionRenderState {
  readonly selectedNodes: ReadonlySet<NodeId>;
  readonly selectedLinks?: ReadonlySet<LinkId>;
  readonly activeNode?: NodeId;
  readonly hoverNode?: NodeId;
  readonly focusedControl?: string;
  readonly hoveredControl?: string;
  readonly focusedRampTarget?: string;
  readonly hoveredRampTarget?: string;
  readonly activeRampStopByControl?: ReadonlyMap<string, string>;
  readonly collapseAnimations?: ReadonlyMap<NodeId, { readonly value: number }>;
  readonly controlEdit?:
    | { readonly kind: "string"; readonly controlId: string; readonly buffer: string }
    | {
        readonly kind: "number";
        readonly controlId: string;
        readonly component: number;
        readonly buffer: string;
        readonly selectAll: boolean;
      };
  readonly box?: { start: { x: number; y: number }; current: { x: number; y: number } };
  readonly linkDrag?: { from: SocketId; current: { x: number; y: number }; candidate?: SocketId };
  readonly parentHighlight?: NodeId;
}
export interface RenderStats {
  readonly candidateNodes: number;
  readonly totalNodes: number;
  readonly paintedNodes: number;
  readonly candidateLinks: number;
  readonly totalLinks: number;
  readonly paintedLinks: number;
  readonly paintMs: number;
}
export interface ResourceImage {
  readonly bitmap: ImageBitmap;
  readonly name: string;
}

function valueText(value: unknown): string {
  const typed = value as ParameterValue | undefined;
  if (!typed || typeof typed !== "object" || !("kind" in typed)) return "—";
  if (typed.kind === "number") return typed.value.toFixed(3);
  if (typed.kind === "vector" || typed.kind === "color")
    return typed.value.map((component) => component.toFixed(3)).join(" ");
  if (typed.kind === "json") return "…";
  return String(typed.value);
}
function resourceLabel(reference: string, prefix: string | undefined): string {
  if (!prefix || !reference.startsWith(prefix)) return reference;
  try {
    return decodeURIComponent(reference.slice(prefix.length).split(":").at(-1)!);
  } catch {
    return reference;
  }
}

/** Canvas maxWidth scales glyphs but does not constrain them. Clip every label to its owning cell. */
function clippedText(
  context: OffscreenCanvasRenderingContext2D,
  text: string,
  rect: Rect,
  x: number,
  y: number,
  padding = 3,
): void {
  context.save();
  context.beginPath();
  context.rect(rect.x + padding, rect.y, Math.max(0, rect.width - padding * 2), rect.height);
  context.clip();
  context.fillText(text, x, y);
  context.restore();
}

function paintColorSwatch(
  context: OffscreenCanvasRenderingContext2D,
  rect: Rect,
  rgba: readonly number[],
  theme: FxNodeTheme,
  zoom: number,
): void {
  context.save();
  context.beginPath();
  context.roundRect(rect.x, rect.y, rect.width, rect.height, 4 * zoom);
  context.clip();
  const cell = Math.max(3, 5 * zoom);
  for (let y = rect.y; y < rect.y + rect.height; y += cell)
    for (let x = rect.x; x < rect.x + rect.width; x += cell) {
      context.fillStyle =
        (Math.floor((x - rect.x) / cell) + Math.floor((y - rect.y) / cell)) % 2
          ? theme.checkerDark
          : theme.checkerLight;
      context.fillRect(x, y, cell, cell);
    }
  context.fillStyle = `rgba(${(rgba[0] ?? 0) * 255},${(rgba[1] ?? 0) * 255},${(rgba[2] ?? 0) * 255},${rgba[3] ?? 1})`;
  context.fillRect(rect.x, rect.y, rect.width, rect.height);
  context.restore();
  context.beginPath();
  context.roundRect(
    rect.x + 0.5 * zoom,
    rect.y + 0.5 * zoom,
    Math.max(0, rect.width - zoom),
    Math.max(0, rect.height - zoom),
    4 * zoom,
  );
  context.strokeStyle = theme.widgetBorder;
  context.lineWidth = zoom;
  context.stroke();
}

function paintControl(
  context: OffscreenCanvasRenderingContext2D,
  control: LayoutControl,
  rect: Rect,
  theme: FxNodeTheme,
  zoom: number,
  interaction?: InteractionRenderState,
  resourceImages?: ReadonlyMap<string, ResourceImage>,
): void {
  const viewRect = (bounds: Rect): Rect => ({
    x: rect.x + (bounds.x - control.bounds.x) * zoom,
    y: rect.y + (control.bounds.y - bounds.y) * zoom,
    width: bounds.width * zoom,
    height: bounds.height * zoom,
  });
  const typedRamp = control.value as { kind?: unknown; value?: unknown };
  if (control.kind === "color-ramp" && typedRamp?.kind === "json" && isColorRamp(typedRamp.value)) {
    const bounds = control.rampBounds!,
      ramp = typedRamp.value,
      active = ramp.stops.find((s) => s.id === interaction?.activeRampStopByControl?.get(control.id)) ?? ramp.stops[0]!,
      toolbar = viewRect(bounds.toolbar),
      menusY = viewRect(bounds.mode).y,
      gradient = viewRect(bounds.gradient);
    context.textAlign = "center";
    const toolbarButtons = [
      [0, 0.12, "+"],
      [0.12, 0.24, "−"],
      [0.24, 0.62, "Flip"],
      [0.62, 1, "Distribute"],
    ] as const;
    for (const [start, end, label] of toolbarButtons) {
      const button = {
        x: toolbar.x + toolbar.width * start,
        y: toolbar.y,
        width: toolbar.width * (end - start) - 1 * zoom,
        height: toolbar.height,
      };
      context.fillStyle = theme.control;
      context.fillRect(button.x, button.y, button.width, button.height);
      context.fillStyle = theme.text;
      clippedText(context, label, button, button.x + button.width / 2, button.y + button.height / 2);
    }
    const labels = [ramp.colorMode.toUpperCase(), ramp.interpolation.replace("-", " "), ramp.hueInterpolation];
    const menuWidths = [0.3, 0.41, 0.29];
    let x = rect.x;
    for (let i = 0; i < 3; i++) {
      const w = rect.width * menuWidths[i]!;
      context.fillStyle = theme.control;
      context.fillRect(x, menusY, w - 2 * zoom, 20 * zoom);
      context.fillStyle = theme.text;
      context.fillText(labels[i]!, x + w / 2, menusY + 10 * zoom);
      x += w;
    }
    context.save();
    context.beginPath();
    context.rect(gradient.x, gradient.y, gradient.width, gradient.height);
    context.clip();
    const cell = 6 * zoom;
    for (let yy = gradient.y; yy < gradient.y + gradient.height; yy += cell)
      for (let xx = gradient.x; xx < gradient.x + gradient.width; xx += cell) {
        context.fillStyle =
          (Math.floor((xx - gradient.x) / cell) + Math.floor((yy - gradient.y) / cell)) % 2
            ? theme.checkerDark
            : theme.checkerLight;
        context.fillRect(
          xx,
          yy,
          Math.min(cell, gradient.x + gradient.width - xx),
          Math.min(cell, gradient.y + gradient.height - yy),
        );
      }
    const steps = Math.max(2, Math.ceil(gradient.width));
    for (let i = 0; i < steps; i++) {
      const c = sampleColorRamp(ramp, i / (steps - 1));
      context.fillStyle = `rgba(${c[0] * 255},${c[1] * 255},${c[2] * 255},${c[3]})`;
      context.fillRect(
        gradient.x + (i * gradient.width) / steps,
        gradient.y,
        gradient.width / steps + 1,
        gradient.height,
      );
    }
    context.restore();
    context.strokeStyle = theme.rampBorder;
    context.lineWidth = 1;
    context.strokeRect(
      gradient.x + 0.5,
      gradient.y + 0.5,
      Math.max(0, gradient.width - 1),
      Math.max(0, gradient.height - 1),
    );
    for (const stop of ramp.stops) {
      const sx = gradient.x + stop.position * gradient.width,
        sy = gradient.y + gradient.height;
      context.beginPath();
      context.moveTo(sx, sy);
      context.lineTo(sx - 6 * zoom, sy + 12 * zoom);
      context.lineTo(sx + 6 * zoom, sy + 12 * zoom);
      context.closePath();
      context.fillStyle = `rgb(${stop.color[0] * 255},${stop.color[1] * 255},${stop.color[2] * 255})`;
      context.fill();
      context.strokeStyle = stop.id === active.id ? theme.focus : theme.emphasis;
      context.lineWidth = stop.id === active.id ? 3 : 2;
      context.stroke();
    }
    const detailsY = viewRect(bounds.selector).y,
      widths = [0.25, 0.35, 0.4],
      details = [active.id, `Pos ${active.position.toFixed(3)}`, ""];
    let dx = rect.x;
    for (let i = 0; i < 3; i++) {
      const w = rect.width * widths[i]!;
      const cellRect = { x: dx, y: detailsY, width: w - 2 * zoom, height: 20 * zoom };
      context.fillStyle = theme.control;
      context.fillRect(cellRect.x, cellRect.y, cellRect.width, cellRect.height);
      context.fillStyle = theme.text;
      clippedText(context, details[i]!, cellRect, cellRect.x + cellRect.width / 2, cellRect.y + cellRect.height / 2);
      dx += w;
    }
    const color = viewRect(bounds.color);
    paintColorSwatch(context, color, active.color, theme, zoom);
    if (interaction?.focusedControl === control.id) {
      context.strokeStyle = theme.focus;
      context.strokeRect(rect.x, detailsY, rect.width, 20 * zoom);
    }
    context.textAlign = "left";
    return;
  }
  context.beginPath();
  context.roundRect(rect.x, rect.y, rect.width, rect.height, 4 * zoom);
  context.fillStyle = control.linked ? theme.body : theme.control;
  context.fill();
  if (interaction?.focusedControl === control.id && !control.numericFields.length) {
    context.strokeStyle = theme.focus;
    context.stroke();
  }
  context.fillStyle = theme.text;
  context.textAlign = "center";
  if (control.kind === "resource") {
    const typed = control.value as { kind?: unknown; value?: unknown },
      reference = typed?.kind === "string" && typeof typed.value === "string" ? typed.value : "",
      image = resourceImages?.get(reference),
      preview = control.resourceBounds ? viewRect(control.resourceBounds.preview) : undefined;
    if (preview) {
      context.save();
      context.beginPath();
      context.roundRect(preview.x, preview.y, preview.width, preview.height, 4 * zoom);
      context.clip();
      context.fillStyle = theme.resourceBackground;
      context.fillRect(preview.x, preview.y, preview.width, preview.height);
      if (image) {
        const scale = Math.min(preview.width / image.bitmap.width, preview.height / image.bitmap.height),
          width = image.bitmap.width * scale,
          height = image.bitmap.height * scale;
        context.drawImage(
          image.bitmap,
          preview.x + (preview.width - width) / 2,
          preview.y + (preview.height - height) / 2,
          width,
          height,
        );
      } else {
        context.fillStyle = theme.muted;
        clippedText(
          context,
          reference ? "Image unavailable — reopen" : "No image",
          preview,
          preview.x + preview.width / 2,
          preview.y + preview.height / 2,
          4 * zoom,
        );
      }
      context.restore();
      context.strokeStyle = theme.widgetBorder;
      context.strokeRect(
        preview.x + 0.5,
        preview.y + 0.5,
        Math.max(0, preview.width - 1),
        Math.max(0, preview.height - 1),
      );
    }
    const label = image?.name || resourceLabel(reference, control.resourceReferencePrefix),
      open = control.openTitle ?? "Open";
    clippedText(
      context,
      label ? `${label}   ${open}…` : `${open} Image…`,
      rect,
      rect.x + rect.width / 2,
      rect.y + rect.height / 2,
      3 * zoom,
    );
    context.textAlign = "left";
    return;
  }
  if (control.kind === "color") {
    const value = control.value as Extract<ParameterValue, { kind: "color" }>;
    paintColorSwatch(context, rect, value.value, theme, zoom);
    if (interaction?.focusedControl === control.id) {
      context.beginPath();
      context.roundRect(
        rect.x + 0.5 * zoom,
        rect.y + 0.5 * zoom,
        Math.max(0, rect.width - zoom),
        Math.max(0, rect.height - zoom),
        4 * zoom,
      );
      context.strokeStyle = theme.focus;
      context.stroke();
    }
    context.textAlign = "left";
    return;
  }
  const edit = interaction?.controlEdit?.controlId === control.id ? interaction.controlEdit : undefined;
  const text = edit?.kind === "string" ? `${edit.buffer}|` : valueText(control.value);
  if (control.numericFields.length) {
    const typed = control.value as Extract<ParameterValue, { kind: "number" | "vector" | "color" }>;
    for (const field of control.numericFields) {
      const fieldRect = viewRect(field.bounds),
        valueRect = viewRect(field.value),
        decrement = viewRect(field.decrement),
        increment = viewRect(field.increment),
        component = typed.kind === "number" ? typed.value : (typed.value[field.component] ?? 0),
        subfield = control.subfields.find((item) => item.index === field.component),
        activeEdit = edit?.kind === "number" && edit.component === field.component ? edit : undefined;
      context.beginPath();
      context.roundRect(fieldRect.x, fieldRect.y, fieldRect.width, fieldRect.height, 4 * zoom);
      context.fillStyle = activeEdit ? theme.controlEditing : control.linked ? theme.body : theme.control;
      context.fill();
      if (!activeEdit && field.range) {
        const ratio = Math.max(
          0,
          Math.min(1, (component - field.range.minimum) / (field.range.maximum - field.range.minimum)),
        );
        context.save();
        context.beginPath();
        context.roundRect(fieldRect.x, fieldRect.y, fieldRect.width, fieldRect.height, 4 * zoom);
        context.clip();
        context.fillStyle = theme.controlFill;
        context.fillRect(fieldRect.x, fieldRect.y, fieldRect.width * ratio, fieldRect.height);
        context.restore();
      }
      if (activeEdit) {
        const x = fieldRect.x + 7 * zoom,
          y = fieldRect.y + fieldRect.height / 2,
          textWidth = context.measureText(activeEdit.buffer).width;
        context.textAlign = "left";
        if (activeEdit.selectAll) {
          context.fillStyle = theme.textSelection;
          context.fillRect(
            x - 2 * zoom,
            fieldRect.y + 2 * zoom,
            Math.min(textWidth + 4 * zoom, fieldRect.width - 10 * zoom),
            fieldRect.height - 4 * zoom,
          );
        }
        context.fillStyle = theme.text;
        clippedText(context, activeEdit.buffer, fieldRect, x, y, 5 * zoom);
        if (!activeEdit.selectAll) {
          const caretX = Math.min(fieldRect.x + fieldRect.width - 5 * zoom, x + textWidth + 1 * zoom);
          context.fillRect(caretX, fieldRect.y + 4 * zoom, 1 * zoom, fieldRect.height - 8 * zoom);
        }
      } else {
        context.fillStyle = theme.muted;
        for (const [button, direction] of [
          [decrement, -1],
          [increment, 1],
        ] as const) {
          const cx = button.x + button.width / 2,
            cy = button.y + button.height / 2,
            size = Math.min(3 * zoom, button.width * 0.28);
          context.beginPath();
          context.moveTo(cx + direction * size, cy);
          context.lineTo(cx - direction * size, cy - size);
          context.lineTo(cx - direction * size, cy + size);
          context.closePath();
          context.fill();
        }
        context.fillStyle = theme.text;
        context.textAlign = "left";
        clippedText(
          context,
          subfield?.label ?? control.label,
          valueRect,
          valueRect.x + 3 * zoom,
          fieldRect.y + fieldRect.height / 2,
          2 * zoom,
        );
        context.textAlign = "right";
        clippedText(
          context,
          component.toFixed(3),
          valueRect,
          valueRect.x + valueRect.width - 3 * zoom,
          fieldRect.y + fieldRect.height / 2,
          2 * zoom,
        );
      }
      context.beginPath();
      context.roundRect(
        fieldRect.x + 0.5 * zoom,
        fieldRect.y + 0.5 * zoom,
        Math.max(0, fieldRect.width - zoom),
        Math.max(0, fieldRect.height - zoom),
        4 * zoom,
      );
      context.strokeStyle = activeEdit ? theme.editOutline : theme.outline;
      context.lineWidth = zoom;
      context.stroke();
    }
  } else if (control.subfields.length) {
    const typed = control.value as Extract<ParameterValue, { kind: "vector" | "color" }>;
    for (const field of control.subfields) {
      const fieldRect = {
        x: rect.x + (field.bounds.x - control.bounds.x) * zoom,
        y: rect.y,
        width: field.bounds.width * zoom,
        height: rect.height,
      };
      context.beginPath();
      context.roundRect(fieldRect.x, fieldRect.y, fieldRect.width, fieldRect.height, 3 * zoom);
      context.fillStyle = control.linked ? theme.body : theme.control;
      context.fill();
      const component = typed.value[field.index] ?? 0;
      context.fillStyle = theme.text;
      clippedText(
        context,
        `${field.label} ${component.toFixed(3)}`,
        fieldRect,
        fieldRect.x + fieldRect.width / 2,
        rect.y + rect.height / 2,
        2 * zoom,
      );
    }
  } else {
    clippedText(context, text, rect, rect.x + rect.width / 2, rect.y + rect.height / 2, 3 * zoom);
  }
  context.textAlign = "left";
}

function paintSocket(
  context: OffscreenCanvasRenderingContext2D,
  socket: LayoutSocket,
  theme: FxNodeTheme,
  zoom: number,
  transform: ViewTransform,
  showLabel = true,
): void {
  const point = worldToView(socket.anchor, transform);
  context.fillStyle = socket.color;
  context.beginPath();
  context.arc(point.x, point.y, G.socket * zoom, 0, Math.PI * 2);
  context.fill();
  if (showLabel && zoom >= 0.35) {
    context.fillStyle = theme.text;
    context.textAlign = socket.direction === "input" ? "left" : "right";
    context.fillText(socket.label, point.x + (socket.direction === "input" ? 10 : -10) * zoom, point.y);
  }
  context.textAlign = "left";
}
export function renderCanvas(
  context: OffscreenCanvasRenderingContext2D,
  snapshot: LayoutSnapshot,
  theme: FxNodeTheme,
  interaction?: InteractionRenderState,
  resourceImages?: ReadonlyMap<string, ResourceImage>,
  deviceTarget: DevicePaintTarget = {
    x: 0,
    y: 0,
    width: Math.round(snapshot.transform.viewport.x * snapshot.transform.dpr),
    height: Math.round(snapshot.transform.viewport.y * snapshot.transform.dpr),
  },
): RenderStats {
  const started = performance.now();
  let paintedNodes = 0,
    paintedLinks = 0;
  const planned = snapshot as LayoutSnapshot & {
    candidateLinkIds?: readonly LinkId[];
    candidateNodeIds?: readonly NodeId[];
    totalNodes?: number;
    totalLinks?: number;
  };
  const { transform: t } = snapshot;
  context.setTransform(t.dpr, 0, 0, t.dpr, deviceTarget.x, deviceTarget.y);
  context.fillStyle = theme.background;
  context.fillRect(0, 0, t.viewport.x, t.viewport.y);
  const spacing = G.grid * t.zoom;
  context.fillStyle = theme.grid;
  const phase = worldToView({ x: 0, y: 0 }, t);
  for (let x = ((phase.x % spacing) + spacing) % spacing; x < t.viewport.x; x += spacing)
    for (let y = ((phase.y % spacing) + spacing) % spacing; y < t.viewport.y; y += spacing) {
      context.beginPath();
      context.arc(x, y, t.zoom < 0.7 ? 0.5 : 1, 0, Math.PI * 2);
      context.fill();
    }
  const viewRect = (rect: Rect) => {
    const p = worldToView({ x: rect.x, y: rect.y }, t);
    return { x: p.x, y: p.y, width: rect.width * t.zoom, height: rect.height * t.zoom };
  };
  for (const id of snapshot.drawOrder) {
    const node = snapshot.nodes.get(id);
    if (!node?.visible || node.kind !== "frame") continue;
    const r = viewRect(node.bounds),
      radius = 10 * t.zoom,
      h = G.header * t.zoom;
    context.beginPath();
    context.roundRect(r.x, r.y, r.width, r.height, radius);
    context.fillStyle = theme.frame;
    context.fill();
    context.strokeStyle = theme.frameHeader;
    context.lineWidth = 1;
    context.stroke();
    context.fillStyle = theme.frameHeader;
    context.font = `600 ${13 * t.zoom}px sans-serif`;
    context.textBaseline = "middle";
    if (t.zoom >= 0.25) context.fillText(node.label, r.x + 10 * t.zoom, r.y + h / 2);
    context.beginPath();
    context.moveTo(r.x + 8 * t.zoom, r.y + h);
    context.lineTo(r.x + r.width - 8 * t.zoom, r.y + h);
    context.stroke();
  }
  context.lineWidth = 2;
  for (const id of planned.candidateLinkIds ?? snapshot.links.keys()) {
    const link = snapshot.links.get(id);
    if (!link?.visible) continue;
    paintedLinks++;
    const color = link.muted ? theme.linkMuted : link.color,
      start = worldToView(link.points[0]!, t),
      end = worldToView(link.points.at(-1)!, t),
      c1 = worldToView(link.controls[0], t),
      c2 = worldToView(link.controls[1], t);
    const emphasized = interaction?.selectedLinks?.has(link.id) || interaction?.knife?.crossed.has(link.id);
    context.strokeStyle = emphasized ? theme.emphasis : color;
    context.lineWidth = emphasized ? 4 : 2;
    context.beginPath();
    context.moveTo(start.x, start.y);
    context.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, end.x, end.y);
    context.stroke();
  }
  for (const id of snapshot.drawOrder) {
    const node = snapshot.nodes.get(id);
    if (!node?.visible || node.kind === "frame") continue;
    if (node.kind === "reroute") {
      paintedNodes++;
      const row = node.rows.find((item) => item.kind === "socket"),
        socket = row?.kind === "socket" ? snapshot.sockets.get(row.socketId) : undefined;
      if (socket) {
        const p = worldToView(socket.anchor, t),
          selected = interaction?.selectedNodes.has(node.id);
        if (selected) {
          context.beginPath();
          context.arc(p.x, p.y, (G.reroute + 5) * t.zoom, 0, Math.PI * 2);
          context.strokeStyle = node.id === interaction?.activeNode ? theme.nodeActive : theme.nodeSelected;
          context.lineWidth = 2;
          context.stroke();
        }
        context.fillStyle = socket.color;
        context.beginPath();
        context.arc(p.x, p.y, G.reroute * t.zoom, 0, Math.PI * 2);
        context.fill();
      }
      continue;
    }
    paintedNodes++;
    const r = viewRect(node.bounds),
      h = G.header * t.zoom;
    const radius = G.corner * t.zoom;
    context.shadowColor = theme.shadow;
    context.shadowBlur = 8;
    context.shadowOffsetY = 3;
    context.beginPath();
    context.roundRect(r.x, r.y, r.width, r.height, radius);
    context.fillStyle = theme.body;
    context.fill();
    context.shadowColor = "transparent";
    context.save();
    context.beginPath();
    context.roundRect(r.x, r.y, r.width, r.height, radius);
    context.clip();
    context.fillStyle = node.headerColor;
    context.fillRect(r.x, r.y, r.width, h);
    context.restore();
    context.beginPath();
    context.roundRect(r.x, r.y, r.width, r.height, radius);
    context.strokeStyle = theme.outline;
    context.lineWidth = 1;
    context.stroke();
    context.fillStyle = theme.text;
    context.font = `600 ${12 * t.zoom}px sans-serif`;
    context.textBaseline = "middle";
    if (t.zoom >= 0.25) context.fillText(node.label, r.x + 22 * t.zoom, r.y + h / 2);
    const cx = r.x + 10 * t.zoom,
      cy = r.y + h / 2,
      collapseAmount = Math.max(
        0,
        Math.min(1, interaction?.collapseAnimations?.get(node.id)?.value ?? (node.collapsed ? 1 : 0)),
      );
    context.save();
    context.translate(cx, cy);
    context.rotate((-Math.PI / 2) * collapseAmount);
    context.beginPath();
    context.moveTo(-5 * t.zoom, -2.5 * t.zoom);
    context.lineTo(0, 2.5 * t.zoom);
    context.lineTo(5 * t.zoom, -2.5 * t.zoom);
    context.strokeStyle = "rgba(255,255,255,.75)";
    context.lineWidth = 2 * t.zoom;
    context.lineCap = "round";
    context.lineJoin = "round";
    context.stroke();
    context.restore();
    if (interaction?.selectedNodes.has(node.id)) {
      context.beginPath();
      context.roundRect(r.x, r.y, r.width, r.height, radius);
      context.strokeStyle = node.id === interaction.activeNode ? theme.nodeActive : theme.nodeSelected;
      context.lineWidth = 2;
      context.stroke();
    }
    context.font = `${11 * t.zoom}px sans-serif`;
    for (const row of node.rows) {
      if (
        row.kind === "header" ||
        row.kind === "category" ||
        row.kind === "section" ||
        row.kind === "panel" ||
        row.kind === "placeholder"
      ) {
        const rowRect = viewRect(row.bounds);
        context.fillStyle = theme.muted;
        context.fillText(row.label, rowRect.x + 8 * t.zoom, rowRect.y + rowRect.height / 2);
      } else if (row.kind === "control") {
        const control = snapshot.controls.get(row.controlId);
        if (control) {
          const rowRect = viewRect(row.bounds);
          if (control.kind !== "resource" && control.numericFields.length !== 1) {
            context.fillStyle = theme.muted;
            context.fillText(control.label, rowRect.x + 8 * t.zoom, rowRect.y + rowRect.height / 2);
          }
          paintControl(context, control, viewRect(control.bounds), theme, t.zoom, interaction, resourceImages);
        }
      } else if (row.kind === "grading-wheels") {
        for (const wheel of row.wheels) {
          const scalar = snapshot.controls.get(wheel.scalarControlId),
            color = snapshot.controls.get(wheel.colorControlId),
            value = color?.value as { kind?: unknown; value?: readonly number[] };
          if (!scalar || !color || value?.kind !== "color" || !value.value || !color.colorWheelBounds) continue;
          const label = viewRect(wheel.labelBounds),
            plane = viewRect(color.colorWheelBounds.plane),
            lightness = viewRect(color.colorWheelBounds.lightness),
            model = oklabToOklch(srgbToOklab([value.value[0] ?? 1, value.value[1] ?? 1, value.value[2] ?? 1]));
          context.fillStyle = theme.muted;
          context.textAlign = "center";
          context.fillText(wheel.label, label.x + label.width / 2, label.y + label.height / 2);
          paintOklchWheel(
            context,
            { plane, lightness },
            model,
            t.dpr,
            Math.min(0.9, Math.max(0.1, model.l)),
            deviceTarget,
          );
          paintControl(context, scalar, viewRect(scalar.bounds), theme, t.zoom, interaction);
          context.textAlign = "left";
        }
      } else if (row.kind === "socket") {
        const control = row.controlId ? snapshot.controls.get(row.controlId) : undefined;
        const socket = snapshot.sockets.get(row.socketId),
          embedsLabel = !!control && !control.linked && control.numericFields.length === 1;
        if (socket) paintSocket(context, socket, theme, t.zoom, t, !embedsLabel);
        if (control && !control.linked)
          paintControl(context, control, viewRect(control.bounds), theme, t.zoom, interaction);
      }
    }
    if (node.muted) {
      context.save();
      context.beginPath();
      context.roundRect(r.x, r.y, r.width, r.height, radius);
      context.clip();
      context.fillStyle = theme.muteOverlay;
      context.fillRect(r.x, r.y, r.width, r.height);
      context.restore();
      for (const bypass of node.bypasses) {
        const a = worldToView(bypass.from, t),
          b = worldToView(bypass.to, t),
          dx = Math.max(20, Math.abs(b.x - a.x) * 0.4);
        context.strokeStyle = theme.linkMuted;
        context.lineWidth = 3;
        context.beginPath();
        context.moveTo(a.x, a.y);
        context.bezierCurveTo(a.x + dx, a.y, b.x - dx, b.y, b.x, b.y);
        context.stroke();
      }
    }
    if (!node.collapsed) {
      context.beginPath();
      context.moveTo(r.x + r.width - 9 * t.zoom, r.y + r.height);
      context.lineTo(r.x + r.width, r.y + r.height - 9 * t.zoom);
      context.strokeStyle = theme.resize;
      context.lineWidth = 1;
      context.stroke();
    }
    context.textAlign = "left";
  }
  if (interaction?.parentHighlight) {
    const n = snapshot.nodes.get(interaction.parentHighlight);
    if (n) {
      const r = viewRect(n.bounds);
      context.strokeStyle = theme.focus;
      context.lineWidth = 3;
      context.strokeRect(r.x, r.y, r.width, r.height);
    }
  }
  if (interaction?.linkDrag) {
    const from = [...snapshot.sockets.values()].find((socket) => socket.id === interaction.linkDrag?.from);
    if (from) {
      const a = worldToView(from.anchor, t),
        b = interaction.linkDrag.current,
        dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
      context.strokeStyle = from.color;
      context.lineWidth = 2;
      context.beginPath();
      context.moveTo(a.x, a.y);
      context.bezierCurveTo(a.x + dx, a.y, b.x - dx, b.y, b.x, b.y);
      context.stroke();
      for (const socket of snapshot.sockets.values()) {
        if (layoutSocketsCompatible(from, socket)) {
          const p = worldToView(socket.anchor, t);
          context.beginPath();
          context.arc(p.x, p.y, socket.id === interaction.linkDrag.candidate ? 10 : 8, 0, Math.PI * 2);
          context.strokeStyle = socket.id === interaction.linkDrag.candidate ? theme.emphasis : socket.color;
          context.stroke();
        }
      }
    }
  }
  if (interaction?.knife) {
    const points = interaction.knife.points;
    context.strokeStyle = interaction.knife.mode === "mute" ? theme.knifeMuted : theme.emphasis;
    context.lineWidth = 2;
    context.beginPath();
    points.forEach((p, i) => (i ? context.lineTo(p.x, p.y) : context.moveTo(p.x, p.y)));
    context.stroke();
    const p = points.at(-1);
    if (p) {
      context.beginPath();
      context.moveTo(p.x - 7, p.y - 7);
      context.lineTo(p.x + 7, p.y + 7);
      context.moveTo(p.x + 7, p.y - 7);
      context.lineTo(p.x - 7, p.y + 7);
      context.stroke();
    }
  }
  if (interaction?.box) {
    const a = interaction.box.start,
      b = interaction.box.current,
      x = Math.min(a.x, b.x),
      y = Math.min(a.y, b.y),
      w = Math.abs(a.x - b.x),
      h = Math.abs(a.y - b.y);
    context.fillStyle = theme.boxSelectionFill;
    context.fillRect(x, y, w, h);
    context.strokeStyle = theme.focus;
    context.lineWidth = 1;
    context.strokeRect(x + 0.5, y + 0.5, w, h);
  }
  return {
    candidateNodes: planned.candidateNodeIds?.length ?? snapshot.drawOrder.length,
    totalNodes: planned.totalNodes ?? snapshot.nodes.size,
    paintedNodes,
    candidateLinks: planned.candidateLinkIds?.length ?? snapshot.links.size,
    totalLinks: planned.totalLinks ?? snapshot.links.size,
    paintedLinks,
    paintMs: performance.now() - started,
  };
}
