# fxnode upstream provenance

- Source: https://github.com/Heaust-ops/fxnode
- Commit: `618c4e02265568d542350902217641d1bbf1ef40`
- Tree: `03fd4246af80f2e4c855369c5dcf362157c6ef58`
- Imported: 2026-07-29
- License: MIT (see `LICENSE` and `NOTICE.md`)
- Local patches: none

This directory is a committed source snapshot. Application adaptations belong outside
`vendor/fxnode`; do not modify vendored source for integration convenience.

## Reimport

```sh
git clone --filter=blob:none https://github.com/Heaust-ops/fxnode /tmp/fxnode
git -C /tmp/fxnode fetch --depth=1 origin 618c4e02265568d542350902217641d1bbf1ef40
git -C /tmp/fxnode checkout --detach 618c4e02265568d542350902217641d1bbf1ef40
rm -rf vendor/fxnode
mkdir -p vendor/fxnode
git -C /tmp/fxnode archive HEAD | tar -x -C vendor/fxnode
```

After importing, restore this file with the new commit/tree hashes and document any
unavoidable local patches explicitly.
