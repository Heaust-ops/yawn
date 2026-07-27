# Browser support

The certified functional matrix is Chromium and Firefox from Playwright 1.61.1 on desktop Linux. Chromium image goldens are regression tests, not cross-engine or Blender parity tests. WebKit, Safari-branded builds, and mobile are not certified.

The main thread requires module `Worker`, `crypto.randomUUID`, and Canvas 2D. The worker requires `OffscreenCanvas`, a 2D context, cropped `createImageBitmap`, and `ImageBitmap.close`; there is no fallback. Named capability errors identify missing features.

Cross-origin isolation enables an optional `SharedArrayBuffer` pointer lane per view. Without it, normal `postMessage` transport remains functional. Limits include 16 attached views, DPR 4, 8192 logical pixels per dimension, 16,777,216 device pixels per view, 67,108,864 device pixels across all views, and history 1,000.
