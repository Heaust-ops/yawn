# fxnode orb workflow

This repository is a TypeScript node-editor library with a browser client and worker-owned authority. This guide is
the operating checklist for Amp orbs and future implementation threads.

## Environment and services

- No `.env` file or secrets are required for development or tests.
- `.agents/setup` installs the exact `package-lock.json` dependency tree, Chromium, Firefox, and their Linux runtime
  dependencies. It then typechecks the repository and starts declared services.
- `.agents/resume` only reconciles declared services. Keep it fast and idempotent; do not install dependencies there.
- `.amp/services.yaml` declares the examples gallery. Use `amp orb services ensure` to start it and obtain its portal.
  Inspect it with `amp orb service status examples` and `amp orb service logs examples`.
- Run `npm run examples` outside an orb when a local Vite server is sufficient. Vite uses `examples/` as its root.
- Generated `.amp/portals/` manifests and resume logs are local runtime state and must not be committed.

## Architectural invariants

Preserve these unless the user explicitly requests an architectural change:

1. The main-thread root is a message-passing facade. Graph state, composition, history, commands, selection behavior,
   and rendering authority live in the worker.
2. A root can be headless or own multiple attached views. Views share graph/history while camera, selection, pointer
   lane, host requests, and render barriers remain view-local.
3. The worker lazily owns one `OffscreenCanvas` atlas and one retained worker 2D context. Views occupy atlas slots;
   paint and cropped bitmap production are serialized. The final detach releases the atlas.
4. Main-thread HTML canvases necessarily have one presentation context each. Do not describe the design as one
   context across the entire browser application.
5. RAF polling remains continuous for responsive shared-pointer-lane input, but layout, atlas paint, bitmap creation,
   worker frame messages, and presentation remain dirty/on-demand.
6. `FxNodeView.setViewport()` is acknowledged and transactional. Runtime hosts await it before changing canvas backing
   dimensions. Surface generations and dimensions reject stale or incoherent frames.
7. The core library does not register DOM listeners, observers, menus, modals, or file pickers. Those are application
   policy. The example browser host demonstrates explicit cleanup and opt-in detach-on-disconnect behavior.
8. Composition definitions are collections installed through `setTheme`, `setHeaderStyles`, `composeSocket`, and
   `composeNode`; initial graph layout is applied afterward with `setState`.
9. Durable save/load uses command journals plus save-time composition. `setState`/`getState` are portable layout/state
   APIs, not the preferred persistence mechanism.
10. Commands distinguish durable graph mutations from transient UI previews and provide undo/redo semantics.

The implementation history and Oracle-reviewed atlas/multi-view decisions are in the originating
[Amp thread](https://ampcode.com/threads/T-019f806f-11fe-74ef-a1f1-90933c1dc543). Treat the current code, tests, README,
and authored docs as authoritative when they differ from historical discussion.

## Design and implementation process

1. Read the nearest code, applicable `AGENTS.md`, public types, protocol validators, and focused tests before editing.
2. Prefer the smallest change at the existing ownership boundary. Remove obsolete paths rather than layering adapters.
3. For cross-layer architectural work, ask Oracle for a phased plan. Before each phase ask for concrete implementation
   details; after each phase ask for a high-confidence blocker review. Address blockers before moving on.
4. Keep protocol boundaries exact and hostile-safe. A public type change normally requires protocol validation, client,
   worker, declaration/type tests, docs, and real-worker browser coverage.
5. Keep DOM policy outside `src/browser/client.ts`. Extend application hosts or examples instead of secretly attaching
   listeners in `attachView()`.
6. For rendering changes, reason explicitly about device/logical coordinates, DPR, atlas slot clipping, transforms,
   `putImageData` ignoring clip/CTM, bitmap ownership/closing, frame ACKs, and context-loss generations.
7. Do not revert unrelated worktree changes. Do not commit generated `.amp` runtime files. Commit/push only when asked.

## Verification ladder

Choose the narrowest useful checks while iterating, then broaden according to blast radius.

### Fast and focused

```sh
npx prettier --write <touched-files>
npm run typecheck -- --pretty false
node --import tsx --test test/<focused>.test.ts
npx playwright test test/browser/<focused>.spec.ts --config playwright.config.ts --project chromium
```

For multi-view or worker lifecycle changes, run focused tests in both supported engines:

```sh
npx playwright test \
  test/browser/client-multiview.spec.ts \
  test/browser/worker-multiview.spec.ts \
  --config playwright.config.ts
```

### Test groups

```sh
npm test                       # Node unit, protocol, layout, persistence, atlas, and scheduler tests
npm run test:browser           # Real browser behavior in Chromium and Firefox
npm run test:visual            # Core Chromium screenshot baselines
npm run test:examples:visual   # Documentation/example screenshots
npm run check:performance      # Large-layout performance budgets
npm run check:docs             # TypeDoc generation plus VitePress build
npm run check:readme           # README structure and link checks
npm run check:package          # Packed-package and worker-asset smoke test
npm run build                  # Vite library bundle and declaration build
```

### Full release gate

Run this after shared contracts, architecture, rendering, examples, persistence, or release-facing docs change:

```sh
npm run release:check
git diff --check
npm run format:check
```

`release:check` includes typecheck, all unit/browser/visual/example tests, fixture and reference checks, performance,
build, docs, README, composition, and package smoke verification.

## Visual review policy

- Never update screenshots merely to make a failure green.
- Inspect expected, actual, and diff images under `test-results/`. Confirm whether changes are intended and localized.
- Use `npm run test:visual:update` only for reviewed core baseline changes.
- Use `npm run test:examples:visual:update` only for reviewed example/documentation image changes.
- Re-run the corresponding non-update command after refreshing a baseline.
- Save one-off user-review screenshots under `.amp/in/artifacts/`; keep transient inspection images in `test-results/`.

## Documentation expectations

- README and VitePress learning docs explain user workflows and ownership boundaries.
- TypeDoc is the API contract; regenerate it with `npm run docs:api` after public type changes.
- Examples import library source through `@lib/`, never relative `../src` paths.
- Keep the examples gallery linked to every standalone application and relevant focused scene.
