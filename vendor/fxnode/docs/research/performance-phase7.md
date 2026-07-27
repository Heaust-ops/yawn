<!-- Repository research; excluded from the documentation site. -->

# Phase 7 spatial performance

The deterministic stress workload contains exactly **5,000 node rectangles** and
**10,000 link rectangles** (seed 7). Nodes are arranged on a sparse 100 × 50
grid and links connect nearby nodes. The query viewport is 1200 × 800 logical
pixels at DPR 2. CI runs `npm run check:performance`; it checks exact totals and
requires at least 90% culling at candidate p95. Timing is reported, never gated.

Measured in the Amp orb on 2026-07-20: Node 20.9.0, Playwright/Chromium 1.49.1,
generic orb CPU (Intel Xeon 2.60 GHz). Across 100 deterministic viewport queries:

| metric         |      result |
| -------------- | ----------: |
| index build    |   32.604 ms |
| candidates p50 | 72 / 15,000 |
| candidates p95 | 75 / 15,000 |
| query p50      |    0.295 ms |
| query p95      |    0.431 ms |
| p95 culling    |      99.50% |

These values are one-orb observations, not performance guarantees. Candidate
counts and the 90% ratio are deterministic; elapsed times vary by host load.
