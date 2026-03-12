# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- Build: `cargo build`
- Run: `cargo run`
- Test: `cargo test`
- Lint: `cargo clippy`
- Run a single test: `cargo test test_name`

## Error handling

- Never use `.unwrap()` or `.expect()`. Propagate errors with `?` instead.
- Use `anyhow::Result` as the default error type unless a more specific error type already exists.
- For functions that can't meaningfully fail, don't wrap in Result — just return the value directly.
- In `main()`, return `anyhow::Result<()>` and use `?` for all fallible operations.

## Architecture

Real-time voxel raytracer using wgpu + winit. Renders a multi-chunk world of 16³ voxel chunks via GPU raymarching in a fullscreen-triangle fragment shader, with procedurally generated textures and a 2D overlay system composited on top.

### Rendering pipeline

1. **Voxel pass** (`voxel_renderer.rs` + `voxels.wgsl`): Draws a single fullscreen triangle. The fragment shader reads camera uniforms (bind group 0), voxel data + chunk info as storage buffers (bind group 1), and the voxel texture atlas + UV map (bind group 2), then raymarches through multiple 16³ chunks.
2. **Overlay pass** (`overlay_renderer.rs` + `overlay.wgsl`): Immediate-mode 2D renderer drawn on top of the voxel pass (uses `LoadOp::Load`). Builds a `DrawList` of textured quads each frame, uploads vertex/index data, and draws with alpha blending. Screen-space coordinates are converted to NDC via a screen-size uniform.

### Key types

- `Renderer` — owns the wgpu surface, device, queue, and both render pipelines. Coordinates frame lifecycle (`begin_frame` → render passes → `submit`).
- `Camera` / `CameraUniforms` — FPS-style camera with yaw/pitch. `CameraUniforms` is a `Pod` struct matching the GPU uniform layout (with explicit padding).
- `Chunk` (`chunk.rs`) — 16³ flat array of `u8` voxel IDs (4096 bytes). Voxel ID 0 = air; IDs 1–4 map to texture tiles (stone, dirt, grass, brick). `generate_test_chunk()` creates terrain with procedural placement.
- `World` (`world.rs`) — `HashMap<[i32; 3], Chunk>` with dirty tracking. `pack_gpu_data()` serializes all chunks into a flat voxel buffer + `GpuChunkInfo` array for the shader.
- `Config` (`config.rs`) — loads `voxels.toml` from CWD (falls back to defaults). Currently configures `chunk_storage_dir` (default `/tmp/voxels`).
- `ChunkManager` (`chunk_manager.rs`) — async chunk loading/eviction system using tokio channels. Manages a sliding window of loaded chunks around the camera, with `ChunkCommand`/`ChunkResult` message passing between the main thread and a background task.
- `VoxelTextureAtlas` / `voxel_textures.rs` — procedurally generates tile textures (noise-based) and packs them into a texture atlas with a `uv_map: [[f32; 4]; 256]` lookup by voxel ID. Tile definitions live in `TILE_DEFS`.
- `DrawList` / `OverlayVertex` / `Texture` / `Rgba` (`overlay.rs`) — immediate-mode 2D drawing primitives. `DrawList` accumulates quads as vertex/index data each frame.
- `OverlayRenderer` (`overlay_renderer.rs`) — manages the overlay pipeline, dynamically grows vertex/index buffers, and handles texture bind group creation.
- `TextureAtlas` / `TextureAtlasBuilder` (`texture_atlas.rs`) — packs multiple sub-images into a single `Texture` with named regions and UV-rect lookups.
- `TextureAtlasFont` (`texture_atlas_font.rs`) — rasterizes a font charset into a `TextureAtlas` via `ab_glyph`, then exposes `draw_text` to emit quads into a `DrawList`.

### App loop

`main.rs` implements `winit::ApplicationHandler`. The `App` struct owns the renderer, camera, world, and input state. Input is processed in `window_event`, camera is updated each frame in `RedrawRequested`, dirty world data is re-uploaded, then both render passes execute.

### Shaders

WGSL shaders are embedded at compile time via `include_str!`. Changes to `.wgsl` files require a rebuild.

### Design specs

Feature specs and their TODO checklists live in `design/specs/`. Each feature has a `<name>.md` (design doc) and `<name>-todos.md` (implementation checklist). These document the incremental development history of the project.

### Key dependencies

- `wgpu` + `winit` — GPU rendering and windowing
- `glam` — vector/matrix math (`Vec3`, `IVec3`, etc.) with bytemuck support for GPU upload
- `tokio` — async runtime for background chunk loading (rt, sync, time features)
- `bytemuck` — zero-copy casting of structs to GPU buffer bytes
- `ab_glyph` — font rasterization for the overlay text system
- `noise` — procedural noise for terrain/texture generation
- `serde` + `toml` — configuration file parsing (`voxels.toml`)
