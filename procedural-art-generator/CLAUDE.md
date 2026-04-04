# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

A Rust/wgpu procedural art generator. Renders 2D procedural art in a window using GPU-accelerated immediate-mode rendering. Currently displays a demo triangle; the goal is to build out procedural art algorithms (color palettes, vector shapes, fonts, grid layouts).

## Commands

- **Build:** `cargo build`
- **Run:** `cargo run`
- **Check (fast):** `cargo check`
- **Logging:** Uses `tracing` with `RUST_LOG` env filter. Default filter shows `warn` globally and `trace` for this crate. Override with `RUST_LOG=debug cargo run`.

## Architecture

The app uses winit for windowing and wgpu for GPU rendering, connected via a synchronous (`pollster::block_on`) initialization flow.

**Rendering pipeline:**
- `GpuState` (`graphics/wgpu_utils.rs`) — owns wgpu surface, device, queue, and config. Handles resize.
- `ImmediateRenderer` (`graphics/immediate/mod.rs`) — batched immediate-mode 2D renderer. Creates a single render pipeline with a WGSL shader that transforms pixel-space coordinates to NDC.
- Drawing flow: `renderer.begin()` → `Frame` → `frame.material(Material)` → `MaterialGuard` (push triangles/quads) → `MaterialGuard` drops (flushes batch) → `Frame` drops (uploads vertices, records render pass, submits).
- Each `Material` variant produces a bind group (uniform buffer + texture + sampler). `Material::Colored` uses a 1x1 white texture so everything goes through the same shader.
- Vertex colors are multiplied by the material color in `MaterialGuard::push()`, and by the texture sample in the fragment shader.

**Vector graphics (`graphics/vector/mod.rs`):**
- Wraps `lyon` for 2D path tessellation into indexed triangle meshes.
- `PathBuilder` — ergonomic wrapper using `glam::Vec2` for pixel-space path construction (move_to, line_to, bezier curves, close).
- `fill()` / `stroke()` — tessellate a path into `Mesh<ColorVertex2D>` (vertices + u16 indices).
- Output is submitted via `MaterialGuard::indexed_triangle_list()`.

**Indexed drawing:** Both `ColorMaterialGuard` and `TextureMaterialGuard` support `indexed_triangle_list(vertices, indices)` for efficient indexed geometry (used by lyon output). Non-indexed and indexed drawing should not be mixed within the same guard.

**Coordinate system:** pixel-space (origin top-left), converted to NDC in the vertex shader via viewport_size uniform.

**Buffer management:** `Buffer::write()` auto-grows with power-of-two sizing when data exceeds capacity.

## Code Style

- Minimize comments. Only add comments on genuinely tricky or non-obvious code.
