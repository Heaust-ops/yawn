export type Sha256 = string & { readonly __sha256: unique symbol };
export type ReferenceStatus = "pending" | "captured";

export interface ResearchBaseline {
  readonly blenderVersion: "4.5.0";
  readonly sourceCommit: string;
  readonly binarySha256: Sha256;
  readonly manualSnapshot: string;
}

export interface PendingReference {
  readonly id: ReferenceId;
  readonly status: "pending";
  readonly relativePath: string;
  readonly reason: string;
}

export interface CapturedReference {
  readonly id: ReferenceId;
  readonly status: "captured";
  readonly relativePath: string;
  readonly sha256: Sha256;
  readonly capturedAt: string;
  readonly captureMethod: "self-captured-blender-window";
  readonly width: number;
  readonly height: number;
}

export type ReferenceRecord = PendingReference | CapturedReference;

export const REFERENCE_IDS = [
  "shader-basic-linked-near",
  "shader-basic-linked-far",
  "shader-selected-active-hover-near",
  "shader-collapsed-near",
  "common-frame-reroute-near",
  "shader-widget-rich-near",
  "shader-socket-gallery-near",
  "geometry-socket-gallery-near",
] as const;

export type ReferenceId = (typeof REFERENCE_IDS)[number];

export interface ReferenceManifest {
  readonly schemaVersion: 1;
  readonly baseline: ResearchBaseline;
  readonly references: readonly ReferenceRecord[];
}
