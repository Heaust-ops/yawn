# Build, Lint, and Test Commands
- `npm run dev`: Recreate the Docker Compose stack and start the Rust/Vite development watchers
- `npm run dev:container`: Start the Rust/Vite watchers inside the development container
- `npm run build`: Build optimized WASM and JS in `dist/` for development
- `npm run build-release`: Build optimized WASM and JS for production
- `cargo check`: Validate Rust sources quickly before full builds
- `cargo fmt`: Format Rust code with rustfmt
- `SNAPSHOT_URL=<portal-url> npm run snapshot-check`: Capture and validate the Procedural and The Manor renderer baselines through production-parity Chromium
- No unit tests currently exist; add them as `*_tests.rs` modules

# Renderer Baseline in Fresh Orbs
- Before renderer implementation or browser validation, read `.amp/amp_chromium.txt` and follow its production-parity WebGPU procedure.
- First launch the demo and headed Chromium as Amp supervised services, attach over CDP before navigating, and use the HTTPS portal URL rather than localhost.
- Capture baseline screenshots of both the Procedural scene and `/themanor.glb` under `.amp/in/artifacts/`. Confirm each scene reports loaded geometry and inspect worker/page console errors before changing renderer code.
- Leave the browser on `about:blank` after capture so Chromium remains ready without continuously rendering or flooding logs.

# Code Style Guidelines
- **Rust 2021 idioms**: Use snake_case for modules, files, functions, and variables
- **Indentation**: 4 spaces (configured in rustfmt)
- **Imports**: Group std library, external crates, then local modules
- **Types**: Use descriptive struct fields and enum variants (e.g., `positions`, `normals`)
- **Error handling**: Use `thiserror` derive macro for custom error types
- **Naming**: Mirror GLTF semantics explicitly in struct fields
- **WGSL shaders**: Keep binding names aligned with Rust bind group layouts
- **JavaScript/TypeScript**: Format with prettier defaults
- **Comments**: Add documentation comments for public APIs using `///`
- **Logging**: Use `log::info!`, `log::error!`, etc. from the log crate, not `println!`
