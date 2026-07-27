# Blender 4.5 research freeze

## Reproducible baseline

- Blender version: **4.5.0**
- Source commit: [`8cb6b388974a817afedf1317ce26f0c75aa5f181`](https://projects.blender.org/blender/blender/src/commit/8cb6b388974a817afedf1317ce26f0c75aa5f181)
- Official binary SHA-256: `1188b95cc12321c770b631939f7c25a096910b6f884a990bf9c0f62d52b38aec`
- Manual snapshot: [`f72fe39427bf150242dd6cfdd94d902e535d2286`](https://projects.blender.org/blender/blender-manual/src/commit/f72fe39427bf150242dd6cfdd94d902e535d2286)

Source citations are commit-pinned so later UI changes cannot silently alter the baseline. Relevant implementation entry points used only for behavioral study are [node drawing](https://projects.blender.org/blender/blender/src/commit/8cb6b388974a817afedf1317ce26f0c75aa5f181/source/blender/editors/space_node/node_draw.cc) and [node editor space](https://projects.blender.org/blender/blender/src/commit/8cb6b388974a817afedf1317ce26f0c75aa5f181/source/blender/editors/space_node/space_node.cc).

## Clean-room observations

The node editor presents a zoomable canvas. Nodes use title bars, body panels, labeled input/output sockets, links, selection/active emphasis, collapse controls, and inline controls where a socket is not linked. Frames group nodes visually; reroutes reshape link paths. Shader and geometry trees share interaction conventions while exposing domain-specific socket types and node content.

fxnode derives only functional observations and independently measured self-captures. No Blender source code, assets, icons, fonts, manual prose, or manual screenshots are copied into this repository. Names required for compatibility and factual citations are retained. See `NOTICE.md` and the typed reference manifest.

## Reference state

The eight requested captures are represented in `src/research/reference-manifest.ts`. They remain truthfully `pending` because this orb had neither Blender nor Xvfb. The normal Phase 1 check validates this state; the strict check fails until genuine captures and metadata are committed.
