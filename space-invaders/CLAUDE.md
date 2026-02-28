# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Code Style

Minimize comments. Only put comments on large blocks of really opaque code (lots of math, bit shifts, dereferences, etc.).

Prefer returning errors to `unwrap` or `expect`. Unless an obvious error type already exists use the `anyhow::Result` type to avoid specifying the error type.

## Commands

```bash
cargo build          # build
cargo run            # run the demo app
cargo test           # run tests
cargo clippy         # lint
```

## Architecture

This is a 2D rendering demo built on winit + wgpu. The code is organized as a small rendering framework with a demo app on top.

### Top-level modules

- **`lib.rs`** — Crate root. Declares all modules, provides `run()` for native, and `wasm_main()` (wasm-bindgen entry) for WASM.
- **`main.rs`** — Native entry point. Sets up tracing then calls `winit_wgpu::run()`.
- **`state_machine.rs`** — The core loop. `App` implements winit's `ApplicationHandler`, owns `GfxState`, and dispatches to the active `State`. `GfxState` holds the wgpu surface/device/queue/config. `StateSwitch` lets states transition to other states or exit.
- **`renderer.rs`** — `Renderer` manages a collection of `Actor`s (mesh + texture + transform), sorts by z-index, and drives `Pipeline2dTextured` each frame.
- **`fps_counter.rs`** — Simple FPS tracking utility.
- **`physics.rs`** — `PhysicsWorld` wraps rapier2d. Manages kinematic and dynamic rigid bodies via `PhysicsActorId`. Exposes `add_actor`, `remove_actor`, `step`, position/velocity setters, and `active_collision_pairs` for game logic.
- **`starfield.rs`** — `StarField` spawns 45 scrolling star actors (two speed layers) for a parallax background effect.
- **`menu.rs`** — `Menu` renders a centered semi-transparent overlay with lines of text. Used for pause and game-over screens.
- **`game.rs`** — `GameState` implements `State`. Full Space Invaders game: player ship, enemy formation (Squid/Crab/Octopus with 2-frame animation), projectiles, collision processing, score, lives HUD, pause/game-over states.

### `gfx/` — low-level GPU abstractions

- **`gfx/buffer.rs`** — `Buffer<V: Pod>` wraps a `wgpu::Buffer`. `new_aligned()` pads element stride to GPU alignment (used for uniform buffers).
- **`gfx/mesh.rs`** — `Mesh<V: Vertex>` owns a vertex `Buffer` and index `Buffer`. Supports `append_triangle` / `append_quad` and `reset()` for reuse.
- **`gfx/texture.rs`** — `Texture` bundles a `wgpu::Texture`, `TextureView`, and `Sampler`. All textures are `Rgba8UnormSrgb`.
- **`gfx/vertex2d.rs`** — `Vertex2d` with `position: Vec2` and `uv: Vec2`. Implements the `Vertex` trait so the pipeline can get its `VertexBufferLayout`.
- **`gfx/pipeline.rs`** — Generic `Pipeline<V: Vertex>` wraps a `wgpu::RenderPipeline`. `ActiveRenderPass` is a RAII guard that holds the render pass and exposes `draw()`.
- **`gfx/pipeline2d_textured.rs`** — Concrete pipeline for textured 2D quads. Uses dynamic uniform buffer offsets so multiple draw calls with different uniforms can share one bind group per frame. `ActiveTexturedRenderPass` wraps `ActiveRenderPass` and tracks the uniform slot index.

### `resources/` — asset loading & content systems

- **`resources/text.rs`** — CPU-side text rasterization using `ab_glyph`. Packs all glyphs for a string into a horizontal atlas texture, then builds a mesh of quads. Reuses existing GPU allocations when the new atlas fits.
- **`resources/texture_atlas.rs`** — `TextureAtlas` packs multiple named images into one GPU texture and returns `UvRect` per name.
- **`resources/sprites.rs`** — Parses `.sprite` files (palette + pixel grid format) and calls `TextureAtlas::new` to build the sprite sheet.

### Shader

`src/shader2d.wgsl` — Single shader used by `Pipeline2dTextured`. Bind group 0: uniform buffer (binding 0, vertex stage), texture (binding 1, fragment), sampler (binding 2, fragment).

### Key Design Patterns

- New vertex types: implement `Pod + Zeroable` via bytemuck derives, then implement the `Vertex` trait with a `VertexBufferLayout`.
- New pipeline types: compose `Pipeline<V>` internally, define a bind group layout, and wrap `ActiveRenderPass` in a custom pass type.
- Resources (images, fonts) are embedded at compile time via `include_bytes!`.
