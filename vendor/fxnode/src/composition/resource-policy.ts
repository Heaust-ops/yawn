import type { FxNodeImageResourceDefinition } from "./types.js";

export const FXNODE_IMAGE_HARD_LIMITS = Object.freeze({
  maxBytes: 33_554_432,
  maxWidth: 8192,
  maxHeight: 8192,
  maxPixels: 16_777_216,
});

export function effectiveImageResourcePolicy(policy: FxNodeImageResourceDefinition): FxNodeImageResourceDefinition {
  const maxWidth = Math.min(policy.maxWidth, FXNODE_IMAGE_HARD_LIMITS.maxWidth),
    maxHeight = Math.min(policy.maxHeight, FXNODE_IMAGE_HARD_LIMITS.maxHeight);
  return Object.freeze({
    ...policy,
    maxBytes: Math.min(policy.maxBytes, FXNODE_IMAGE_HARD_LIMITS.maxBytes),
    maxWidth,
    maxHeight,
    maxPixels: Math.min(policy.maxPixels, FXNODE_IMAGE_HARD_LIMITS.maxPixels, maxWidth * maxHeight),
  });
}
