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

Real-time voxel raytracer using wgpu + winit. Renders a 16x16x16 voxel chunk via GPU raymarching in a fullscreen-triangle fragment shader, with a 2D overlay system composited on top.

### Rendering pipeline

1. **Voxel pass** (`voxel_renderer.rs` + `voxels.wgsl`): Draws a single fullscreen triangle. The fragment shader reads camera uniforms (bind group 0) and chunk voxel data as a storage buffer (bind group 1), then raymarches through the 16³ grid.
2. **Overlay pass** (`overlay_renderer.rs` + `overlay.wgsl`): Immediate-mode 2D renderer drawn on top of the voxel pass (uses `LoadOp::Load`). Builds a `DrawList` of textured quads each frame, uploads vertex/index data, and draws with alpha blending. Screen-space coordinates are converted to NDC via a screen-size uniform.

### Key types

- `Renderer` — owns the wgpu surface, device, queue, and both render pipelines. Coordinates frame lifecycle (`begin_frame` → render passes → `submit`).
- `Camera` / `CameraUniforms` — FPS-style camera with yaw/pitch. `CameraUniforms` is a `Pod` struct matching the GPU uniform layout (with explicit padding).
- `Chunk` — 16³ flat array of `u8` voxel IDs, uploaded to GPU as a 4096-byte storage buffer.
- `DrawList` / `OverlayVertex` / `Texture` / `Rgba` (`overlay.rs`) — immediate-mode 2D drawing primitives. `DrawList` accumulates quads as vertex/index data each frame.
- `OverlayRenderer` (`overlay_renderer.rs`) — manages the overlay pipeline, dynamically grows vertex/index buffers, and handles texture bind group creation.
- `TextureAtlas` / `TextureAtlasBuilder` (`texture_atlas.rs`) — packs multiple sub-images into a single `Texture` with named regions and UV-rect lookups.
- `TextureAtlasFont` (`texture_atlas_font.rs`) — rasterizes a font charset into a `TextureAtlas` via `ab_glyph`, then exposes `draw_text` to emit quads into a `DrawList`.

### App loop

`main.rs` implements `winit::ApplicationHandler`. The `App` struct owns the renderer, camera, chunk, and input state. Input is processed in `window_event`, camera is updated each frame in `RedrawRequested`, then both render passes execute.

### Shaders

WGSL shaders are embedded at compile time via `include_str!`. Changes to `.wgsl` files require a rebuild.
