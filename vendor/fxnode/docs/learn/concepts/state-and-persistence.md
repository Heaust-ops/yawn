# State and persistence

`getState()` and `setState()` exchange exact, process-local state for the currently installed composition. `setState()` is useful for bootstrap and controlled replacement, not historical imports.

`save()` returns the canonical current `GraphLayoutV2`—a compact graph export, not history. For replayable durable storage use `getSaveData()`: its envelope records the canonical baseline, the applied command journal since that baseline, and the effective save-time composition used to establish compatibility. The baseline and journal are composed at save time to verify that they reproduce the exported current graph.

`load()` accepts historical `GraphLayoutV1`, canonical `GraphLayoutV2`, or the save-data envelope. It stages decode, compatibility checks, declarative migrations, and replay before one atomic publication; structured issues identify paths/codes on failure, and rejected input leaves graph, history, and observable state unchanged. A successful graph change publishes the load mutation/snapshot as one commit. Loading an envelope installs its migrated baseline and command journal (including checkpoint placement); if the resulting graph equals current state, the load is a no-op but the validated journal/baseline is still installed for subsequent undo/redo and saves.

Durable `GraphLayoutV2` uses `schemaVersion: 2`; its historical `catalogVersion` field stores composition version. Unknown types and future node versions round-trip as opaque read-only records. Declarative migration edges must form a complete valid route; failures preserve the original opaque payload. Canonical ordering and bounded admission make saves deterministic and hostile inputs reject safely.

In short: **set/get state** for exact current runtime state; **save** for canonical `GraphLayoutV2`; **save data/load** for compatible persistence and replay. Selection, camera, hover, composition revision, and undo/redo internals are not durable graph fields.
