# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

A Rust/wgpu procedural art generator. Renders 2D procedural art in a window using GPU-accelerated immediate-mode rendering. The app displays a grid of parameterized art instances (currently `HueCircle`) with camera controls, hover tooltips, and threaded initialization.

## Commands

- **Build:** `cargo build`
- **Run:** `cargo run`
- **Check (fast):** `cargo check`
- **No tests** — there is no test suite.
- **Logging:** Uses `tracing` with `RUST_LOG` env filter. Default filter shows `warn` globally and `trace` for this crate. Override with `RUST_LOG=debug cargo run`.
- **Rust edition:** 2024

## Architecture

The app uses winit for windowing and wgpu for GPU rendering, connected via a synchronous (`pollster::block_on`) initialization flow.

**App lifecycle (`windowing.rs`):**
- `run()` takes a state factory closure. On window resume: creates window → initializes `GpuState` → creates `ImmediateRenderer` → calls factory to build `AppState`.
- `AppState` trait (`state.rs`): `event()` → `update()` → `render()` loop. `DemoState` in `main.rs` is the current implementation. `StateTransition` supports `Continue`, `Quit`, and `Switch` (swap to a new `AppState`).
- `InputState` tracks keys (HashSet), mouse position, scroll delta, right-click pan.
- Font (`assets/IntelOneMono-Regular.ttf`) is embedded at compile time via `include_bytes!`.

**Rendering pipeline:**
- `GpuState` (`graphics/wgpu_utils.rs`) — owns wgpu surface, device, queue, and config. Handles resize.
- `ImmediateRenderer` (`graphics/immediate/mod.rs`) — batched immediate-mode 2D renderer with two pipelines: color (position + color) and texture (position + texcoord + color). Both use mat4 modelview transform and material color uniform.
- Drawing flow: `renderer.begin(camera_uniforms)` → `Frame` → `frame.color_material()` or `frame.texture_material()` → guard (push triangles/quads/indexed geometry) → guard drops (flushes batch) → `Frame` drops (uploads vertices, records render pass, submits).
- Two render passes per frame: world-space (with camera) and screen-space (orthographic for UI overlay like FPS and tooltips).

**Vector graphics (`graphics/vector/mod.rs`):**
- Wraps `lyon` for 2D path tessellation into indexed triangle meshes.
- `PathBuilder` — ergonomic wrapper using `glam::Vec2` for pixel-space path construction (move_to, line_to, bezier curves, close). Convenience methods: `circle()`, `rect()`.
- `fill()` / `stroke()` — tessellate a path into `Mesh<ColorVertex2D>` (vertices + u16 indices).
- Output is submitted via guard's `indexed_triangle_list()`. Non-indexed and indexed drawing should not be mixed within the same guard.

**Font rendering (`graphics/font.rs`):**
- `TextureAtlasFont` rasterizes ASCII glyphs into a `TextureAtlas` (shelf-packed 1024-wide texture).
- `draw()` renders text with `HAlign`/`VAlign` positioning via `TextureMaterialGuard`.

**Camera (`camera.rs`):**
- `Camera2D` — pan/zoom with world bounds constraints. WASD keys, right-drag, and scroll wheel.
- `uniforms()` returns `[cam_min_x, cam_min_y, cam_size_x, cam_size_y]` for the vertex shader.

**Parameterized graphics system (`parameterized_grid/`):**
- `ParameterizedGraphics` trait — defines `description()`, parameter metadata (`ParamDef`), instance initialization, and rendering into a cell rect. Implement this to add new art types.
- `ParameterizedGraphicsGrid<A>` — lays out instances in a grid, mapping 1 or 2 parameters across axes (`GridParamMapping`). Initializes instances on background threads with a concurrency semaphore, polls results via mpsc channel.
- `ParamValue` / `ParamRange` (`param.rs`) — type-safe parameter system supporting all numeric types with range interpolation (`lerp`), `min_value()`/`max_value()`, and custom formatters.
- `HueCircle` (`hue_circle.rs`) — example implementation: tessellates a colored circle at a given hue.

**egui overlay (`egui_integration.rs`, `menu.rs`):**
- `EguiIntegration` — wraps egui context, winit state, and wgpu renderer. Lives in `App` (windowing.rs), not in `AppState`.
- ESC toggles the overlay. When visible, egui consumes input events so camera controls are blocked.
- `OverlayState` / `MenuPanel` (`menu.rs`) — extensible menu state machine. Add new `MenuPanel` variants for new panels.
- `AppState::overlay_ui()` — called when overlay is visible to draw egui UI. Default no-op.
- Renders as a third pass (after world + screen UI) with `LoadOp::Load`.

**Coordinate system:** pixel-space (origin top-left), converted to NDC in the vertex shader via viewport_size uniform.

**Buffer management:** `Buffer::write()` auto-grows with power-of-two sizing when data exceeds capacity.

## Code Style

- Minimize comments. Only add comments on genuinely tricky or non-obvious code.
