# fxnode upstream provenance

- Source: https://github.com/Heaust-ops/fxnode
- Commit: `4e96585e99742959660d4107b0078c27ff13e708`
- Tree: `4fcafa60557bcbd760d7aa8d8898c0adbe0bb2c8`
- Imported: 2026-07-29
- License: MIT (see `LICENSE` and `NOTICE.md`)
- Local patches: none

This directory is a committed source snapshot. Application adaptations belong outside
`vendor/fxnode`; do not modify vendored source for integration convenience.

## Reimport

```sh
git clone --filter=blob:none https://github.com/Heaust-ops/fxnode /tmp/fxnode
git -C /tmp/fxnode fetch --depth=1 origin 4e96585e99742959660d4107b0078c27ff13e708
git -C /tmp/fxnode checkout --detach 4e96585e99742959660d4107b0078c27ff13e708
rm -rf vendor/fxnode
mkdir -p vendor/fxnode
git -C /tmp/fxnode archive HEAD | tar -x -C vendor/fxnode
```

After importing, restore this file with the new commit/tree hashes and document any
unavoidable local patches explicitly.
