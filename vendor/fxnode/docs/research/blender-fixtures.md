# Capturing Blender references

Use the official Linux Blender 4.5.0 binary whose archive SHA-256 is recorded in the manifest. A graphical X11 session (a disposable Xvfb session is suitable) and a whole-window capture utility are required.

For each of the eight IDs:

1. Start Blender at a deterministic 1440×900 window size with factory settings.
2. Run `blender --python tools/blender/create-reference-fixtures.py -- --fixture <id>` (add `--save /tmp/<id>.blend` if desired).
3. Wait for redraw, keep the entire Blender window—including chrome—visible, and capture it to `docs/research/blender-references/4.5.0/<id>.png`. For the hover fixture, move the pointer over the active node title before capture; this interaction cannot honestly be synthesized by Blender's data API.
4. Record UTC capture time, pixel dimensions, and `sha256sum` in `src/research/reference-manifest.ts`, change status to `captured`, and set capture method to `self-captured-blender-window`.
5. Run `npm run check:references:strict`.

The script validates Blender 4.5.x, rebuilds the current file, creates material or geometry node trees, lays nodes out deterministically, configures editor zoom, and saves only when asked. Generated `.blend` files are ignored and are not reference artifacts.
