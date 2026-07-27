/** DOM-independent keyboard modifiers. */
export interface FxNodeModifiers {
  readonly alt: boolean;
  readonly control: boolean;
  readonly meta: boolean;
  readonly shift: boolean;
}

export type FxNodeInput =
  | {
      readonly kind: "pointer";
      readonly phase: "down" | "move" | "up" | "cancel";
      readonly pointerId: number;
      readonly pointerType: string;
      readonly position: { readonly x: number; readonly y: number };
      readonly button: number;
      readonly buttons: number;
      readonly modifiers: FxNodeModifiers;
    }
  | {
      readonly kind: "wheel";
      readonly position: { readonly x: number; readonly y: number };
      readonly delta: { readonly x: number; readonly y: number };
      readonly modifiers: FxNodeModifiers;
    }
  | {
      readonly kind: "key";
      readonly phase: "down" | "up";
      readonly key: string;
      readonly code: string;
      readonly repeat: boolean;
      readonly modifiers: FxNodeModifiers;
    }
  | { readonly kind: "focus"; readonly phase: "focus" | "blur" }
  | { readonly kind: "outside-pointer"; readonly button: number };

export interface FxNodeViewport {
  readonly width: number;
  readonly height: number;
  readonly dpr: number;
}
/** DOM-independent camera state in graph coordinates. */
export interface FxNodeCamera {
  readonly center: { readonly x: number; readonly y: number };
  readonly zoom: number;
}
export interface AddNodeParams {
  readonly typeId: string;
  readonly viewPosition: { readonly x: number; readonly y: number };
  readonly nodeId?: string;
}
export interface FxNodeActionOptions {
  readonly expectedVersion?: number;
}
export interface FxNodeSelectionSnapshot {
  readonly nodeCount: number;
  readonly linkCount: number;
  readonly canRemove: boolean;
  readonly mute:
    | { readonly enabled: false }
    | { readonly enabled: true; readonly state: "all-muted" | "all-unmuted" | "mixed" };
}
export interface FxNodeAddNodeMenuRequest {
  readonly kind: "add-node-menu";
  readonly viewPosition: { readonly x: number; readonly y: number };
  readonly compositionRevision: number;
}
export interface FxNodeResourceAuthorization {
  readonly viewId: string;
  readonly token: string;
  readonly graphVersion: number;
  readonly compositionRevision: number;
}
export interface FxNodeImageResourceDescriptor {
  readonly id: string;
  readonly kind: "image";
  readonly title: string;
  readonly openTitle: string;
  readonly accept: readonly string[];
  readonly maxBytes: number;
  readonly maxWidth: number;
  readonly maxHeight: number;
  readonly maxPixels: number;
}
export interface FxNodeResourceOpenRequest {
  readonly kind: "resource-open";
  readonly authorization: FxNodeResourceAuthorization;
  readonly resource: FxNodeImageResourceDescriptor;
}
export interface FxNodeResourceData {
  readonly name: string;
  readonly mime: string;
  readonly bytes: ArrayBuffer;
}
export type FxNodeHostRequest = FxNodeAddNodeMenuRequest | FxNodeResourceOpenRequest;
export interface FxNodeHostSnapshot {
  readonly compositionRevision: number;
  readonly colorPickerOpen: boolean;
  readonly selection: FxNodeSelectionSnapshot;
}
