import { REFERENCE_IDS } from "./reference-types.js";
import type { ReferenceManifest, Sha256 } from "./reference-types.js";

const pendingReason = "Capture environment has no Blender 4.5.0 binary or Xvfb display server.";
const root = "docs/research/blender-references/4.5.0";

export const referenceManifest: ReferenceManifest = {
  schemaVersion: 1,
  baseline: {
    blenderVersion: "4.5.0",
    sourceCommit: "8cb6b388974a817afedf1317ce26f0c75aa5f181",
    binarySha256: "1188b95cc12321c770b631939f7c25a096910b6f884a990bf9c0f62d52b38aec" as Sha256,
    manualSnapshot: "f72fe39427bf150242dd6cfdd94d902e535d2286",
  },
  references: REFERENCE_IDS.map((id) => ({
    id,
    status: "pending",
    relativePath: `${root}/${id}.png`,
    reason: pendingReason,
  })),
};
