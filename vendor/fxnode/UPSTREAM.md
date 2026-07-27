# fxnode upstream provenance

- Source: https://github.com/Heaust-ops/fxnode
- Commit: `3f8745717bf4574577be72e9769373475cc300c9`
- Tree: `fb8bf407854b68dad8c0249c9ba63c7fd0bd9332`
- Imported: 2026-07-26
- License: MIT (see `LICENSE` and `NOTICE.md`)
- Local patches: none

This directory is a committed source snapshot. Application adaptations belong outside
`vendor/fxnode`; do not modify vendored source for integration convenience.

## Reimport

```sh
git clone --filter=blob:none https://github.com/Heaust-ops/fxnode /tmp/fxnode
git -C /tmp/fxnode fetch --depth=1 origin 3f8745717bf4574577be72e9769373475cc300c9
git -C /tmp/fxnode checkout --detach 3f8745717bf4574577be72e9769373475cc300c9
rm -rf vendor/fxnode
mkdir -p vendor/fxnode
git -C /tmp/fxnode archive HEAD | tar -x -C vendor/fxnode
```

After importing, restore this file with the new commit/tree hashes and document any
unavoidable local patches explicitly.
