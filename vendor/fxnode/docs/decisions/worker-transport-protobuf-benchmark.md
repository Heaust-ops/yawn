# Phase 3 worker transport Protobuf benchmark

**Status:** NO-GO (measured 2026-07-22). This decision does not authorize a production transport change.

## Question and method

The experiment compared the current browser structured-clone shape with a generated Protobuf-ES schema sent as a transferable `ArrayBuffer`. It ran in headless, real Chromium with a dedicated module worker. The timed Protobuf path included the main-thread object-to-schema adapter, encode, transferable `postMessage`, worker decode and schema-to-object adapter. The structured path included `postMessage` and structured clone. Both paths computed the same FNV-1a checksum over a recursively key-sorted serialization of the reconstructed synthetic payload in the worker. Every Protobuf send asserted immediate transfer detachment.

Fixtures are deterministic multiplications of nodes, links, sockets, parameters, composition definitions and commands. Graph state, snapshot, node, socket, link and envelope fields are generated typed messages (not JSON bytes in Protobuf). Recursive `JsonValue` represents open values and composition/command/save variants. JSON byte counts below are UTF-8 diagnostics, not a measured transport. Pointer SAB, ImageBitmap frames and resource byte payloads are explicitly excluded.

The orb-constrained run used 10 warmups and 50 samples per payload/path (rather than an exhaustive 3×120). Samples were sequential in one browser, structured clone always ran first, and the harness did not retain raw samples for paired analysis. It therefore cannot establish cross-machine confidence or formal results for every row. The correctly directed `state.set` failures are far outside timer resolution and independently make the wholesale migration fail its acceptance gates.

The harness always encoded on the page and decoded in the worker. Consequently, response, snapshot, save-data, mutation, and host-projection rows were synthetic reverse-direction estimates rather than production-faithful worker-to-host measurements. Some of those envelopes also approximated, rather than passed, the current protocol validators. They corroborate the result but are not used as decisive evidence. The graph-heavy `state.set` requests did run in the production direction and used typed graph messages; those are the basis of the decision.

## Environment

- Orb: Linux 6.1.158+, x86_64, 2 reported logical CPUs
- Chromium: HeadlessChrome 149.0.7827.55, Linux; `crossOriginIsolated=false`
- Node 20.9.0, npm 10.9.8, Vite 6.1.0, Playwright 1.61.1
- Exact tools: `@bufbuild/buf@1.72.0`, `@bufbuild/protobuf@2.13.0`, `@bufbuild/protoc-gen-es@2.13.0`
- The one-off benchmark harness and generated output were removed after review; this record preserves its result and limitations.

## Results

Times are milliseconds. Delta is `(structured p95 - protobuf p95) / structured p95`; positive favors Protobuf.

| Payload                      | JSON bytes | clone p50 / p95 | protobuf p50 / p95 | main codec p95 | p95 delta |
| ---------------------------- | ---------: | --------------: | -----------------: | -------------: | --------: |
| tiny command                 |        143 |     0.00 / 0.20 |        0.30 / 0.70 |           0.20 |   -250.0% |
| state.set medium             |    129,422 |     3.70 / 5.40 |      28.00 / 43.30 |          31.30 |   -701.9% |
| state.set large              |    654,168 |   16.80 / 19.50 |    152.40 / 176.70 |         119.20 |   -806.2% |
| snapshot medium              |    129,413 |     3.40 / 6.00 |      26.60 / 45.20 |          27.80 |   -653.3% |
| snapshot large               |    654,159 |   17.40 / 20.40 |    149.70 / 183.70 |         113.70 |   -800.5% |
| document.replaced large      |    654,175 |   18.10 / 25.50 |    149.80 / 175.50 |         106.70 |   -588.2% |
| save-data medium             |    143,961 |     3.90 / 6.90 |      70.10 / 94.40 |          66.70 | -1,268.1% |
| save-data large              |    624,879 |   17.30 / 21.50 |    310.50 / 383.00 |         243.60 | -1,681.4% |
| initial composition + layout |    559,668 |   14.40 / 17.70 |    279.80 / 322.00 |         204.80 | -1,719.2% |
| receipt                      |        152 |     0.10 / 0.20 |        0.20 / 0.30 |           0.20 |    -50.0% |
| error                        |        139 |     0.10 / 0.20 |        0.20 / 0.30 |           0.10 |    -50.0% |
| input                        |        179 |     0.10 / 0.20 |        0.20 / 0.30 |           0.20 |    -50.0% |
| host projection              |     10,084 |     0.30 / 0.30 |        3.50 / 9.00 |           3.10 | -2,900.0% |

The harness build produced 23,989 gzip bytes (417 bytes for its worker entry and 23,572 bytes for its combined codec chunk). This was not a production baseline/delta comparison and did not account for host/worker runtime duplication, so it is only a non-decisive harness-size estimate.

## Thresholds and decision

The planned gates were: graph-heavy p95 improvement ≥20% in at least 3/4 of medium/large state and snapshot cases; no >5% regression on any other large payload; a small-payload deadband (do not decide on sub-millisecond noise); main-thread codec p95 ≤4 ms and no codec task >16.7 ms; worker gzip delta ≤30 KiB and total ≤45 KiB. A noisy threshold crossing would have been **INCONCLUSIVE**. Because only request-direction cases were production-faithful and the bundle comparison was approximate, not every planned gate was formally established.

The result is still **NO-GO**. Both correctly directed medium and large `state.set` requests regressed by multiples, making the planned 3-of-4 graph gate mathematically impossible to pass. Their page-side adapter/encode latency also exceeded both codec limits by wide margins. Tiny-message differences and reverse-direction estimates are not used to strengthen the decision. The generic `JsonValue` portions do not prove every conceivable fully typed schema would be slow, but the typed graph result is sufficient to reject a wholesale migration now.

Production retains structured clone for object-rich commands, composition, state, and events; the existing `SharedArrayBuffer` pointer lane and naturally binary transferables remain the appropriate narrow optimizations. A future packed typed-array format should be considered only for a measured numeric hot path, not as a generic graph protocol.

## Repository outcome

The reviewed harness was intentionally not retained: it approximated several current envelopes, tested only host-to-worker direction, and would have permanently added a generator/runtime dependency tree for a rejected design. This ADR retains the environment, measured table, decisive evidence, and limitations without turning the one-off experiment into a misleading supported benchmark.
