export const FXNODE_VIEW_LIMITS = Object.freeze({
  maxViews: 16,
  maxLogicalDimension: 8192,
  maxDpr: 4,
  maxDeviceDimension: 8192,
  maxDevicePixelsPerView: 16_777_216,
  maxActiveDevicePixels: 16_777_216,
  maxAtlasDimension: 8192,
  maxAtlasPixels: 16_777_216,
  maxInFlightDevicePixels: 16_777_216,
  minZoom: 0.1,
  maxZoom: 4,
});

/** Returns the actual rounded backing-store size, or undefined when unsupported. */
export function fxNodeDevicePixels(
  width: number,
  height: number,
  dpr: number,
): Readonly<{ width: number; height: number }> | undefined {
  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    !Number.isFinite(dpr) ||
    width < 0 ||
    height < 0 ||
    dpr <= 0 ||
    width > FXNODE_VIEW_LIMITS.maxLogicalDimension ||
    height > FXNODE_VIEW_LIMITS.maxLogicalDimension ||
    dpr > FXNODE_VIEW_LIMITS.maxDpr
  )
    return;
  const deviceWidth = Math.max(1, Math.round(width * dpr)),
    deviceHeight = Math.max(1, Math.round(height * dpr));
  if (
    !Number.isSafeInteger(deviceWidth) ||
    !Number.isSafeInteger(deviceHeight) ||
    deviceWidth > FXNODE_VIEW_LIMITS.maxDeviceDimension ||
    deviceHeight > FXNODE_VIEW_LIMITS.maxDeviceDimension ||
    deviceWidth * deviceHeight > FXNODE_VIEW_LIMITS.maxDevicePixelsPerView
  )
    return;
  return Object.freeze({ width: deviceWidth, height: deviceHeight });
}
