# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project rules

- TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.
- README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.
- Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.
- Prefer strict error handling with specific enums over something like anyhow.
- Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Build, run, test

Workspace of three crates: `engine` (platform-agnostic core), `desktop` (native winit/wgpu entry point), `web` (wasm32 entry point built with Trunk).

```
cargo run -p desktop                 # native, debug
cargo run -p desktop --release       # native, release
cd web && trunk serve                # WebGL2, http://localhost:8000
cd web && trunk build --release
cargo test                           # whole workspace; nearly all tests live in engine
```

Run a single test or module by path (tests are `#[cfg(test)] mod tests` colocated with the code they cover):

```
cargo test -p engine physics::collide::tests::slide_along_wall
cargo test -p engine geom::csg           # every test in that module
```

`config.toml` (see README.md for the format) is desktop-only; web always runs with `Config::default()`.

## Architecture

### Engine core vs. platform shells

`engine::run` (engine/src/app.rs) owns the winit `ApplicationHandler`, the async wgpu setup, and the fixed-timestep loop; `desktop` and `web` are thin entry points that just wire up logging/config and call it. The seam between them is `SpawnFn`, a fn pointer for driving the renderer's async init to completion — desktop passes `pollster::block_on`, web passes `wasm_bindgen_futures::spawn_local` — so `engine` never needs `#[cfg(target_arch = "wasm32")]` branching for that concern. wasm32 does still get its own `wgpu` dependency block (engine/Cargo.toml): it forces the GL backend and drops the `webgpu` cargo feature, because leaving it in makes `Instance::new` commit to WebGPU whenever `navigator.gpu` merely exists and never fall back to WebGL2.

`App::update` runs a `FIXED_DT = 1/120s` accumulator (engine/src/app.rs) decoupled from render/display rate; `App::render` runs once per real frame regardless of how many fixed steps just ran. Edge-triggered actions (quit, cycle-mode) and per-frame analog reads (zoom) are read in `handle_actions` outside `step()` specifically because `step()` can run zero, one, or many times per frame.

### Geometry (`engine/src/geom`)

The level's shape is one signed-distance-field CSG tree (`d < 0` = inside/walkable, `grad` = unit direction of increasing `d`), evaluated by two independent code paths that are cross-checked against each other in tests:

- **`primitive.rs`** — leaf shapes (circle, round-rect, capsule) with exact `sample(p) -> {d, grad}`, and `Similarity` (translate + rotate + *uniform* scale only — non-uniform scale can make a composed field overestimate distance, which would be unsafe for the sphere-trace collision below).
- **`csg.rs`** — `Csg`, an append-only arena of `Union`/`Intersect`/`Complement`/`Transform` nodes addressed by `NodeId`. `eval()` computes the analytic distance recursively; `Difference(a, b)` is built as `Intersect(a, Complement(b))`. AABB-based pruning during `eval` is only sound at `Union` nodes (a box gives a lower bound, which is what pruning a min needs).
- **`parts.rs`** — `Level` wraps a `Csg` in the project's required normal form: the root is implicitly `Union(parts)` where each part is a small bounded subtree, never a bare `Complement`. This is what makes point queries cheap — a uniform grid (`PartGrid`) narrows a query to nearby parts before falling back to exact CSG evaluation.
- **`field.rs`** — chunked, resumable baking of the SDF into a texture (`BakeJob::step` does one 16x16 block at a time so a caller can budget bake work per frame). Values are clamped and stored as `f16`. Baking exploits the field's 1-Lipschitz property to skip whole blocks that are provably beyond the clamp range.
- **`contour.rs`** / **`tess.rs`** — an entirely separate path: exact polygon booleans via `i_overlay` (fixed world-scale grid, so neighboring shapes quantize identically) produce contours, which `lyon` tessellates into fill/stroke meshes. Cost scales with primitive count, not world size, so unlike the field this is baked once, not chunked.

### Physics (`engine/src/physics`)

`collide::move_actor` sphere-traces a disk against `Level::eval` (`sphere_trace`), advancing by exactly the field's own clearance value each iteration — provably safe against tunneling because the composed field is a conservative (never-overestimating) distance. A hit produces a `Contact`; remaining movement is projected along the contact normal for wall sliding, up to `MAX_SLIDE_PASSES` per tick. `depenetrate` runs before and after the trace to recover from any starting overlap.

### Rendering (`engine/src/render`)

Two visual representations of the *same* geometry are drawn simultaneously and can be toggled independently via `RenderMode`:

- **Glow** — `ChunkAtlas`/`ChunkPipeline` (chunks.rs) stream the baked field (from `geom::field`) into a `Texture2DArray`, one layer per resident chunk, loading/evicting/baking based on the camera's predicted position each frame under a millisecond budget. An LRU cache holds recently-evicted chunk data so revisiting doesn't re-bake from scratch.
- **Crisp** — `CrispPipeline` draws the exact tessellated fill/stroke meshes from `geom::contour`/`geom::tess`, built once at load. The stroke draws in *every* mode, since it's the visual cross-check between the baked field and the exact geometry, not a crisp-only feature.

Both draw into an HDR-ish `Offscreen` target (`Rgba16Float` if the adapter supports it as a render attachment, else `Rgba8Unorm`) at a configurable `render_scale`; `CompositePipeline` downsamples that into the swapchain with one bilinear tap. HUD text (`text_pipeline.rs`, glyphs from `text/atlas.rs` via `fontdue`) is drawn directly onto the swapchain afterward, unaffected by render scale. `gpu.rs` probes adapter capabilities (HDR attachment support, max texture size) at startup rather than assuming them, since they differ between native Vulkan and WebGL2.

`render/streaming_fixture.rs` is a hand-authored, deterministic multi-room/corridor level used to exercise chunk streaming — not a level generator.

### Input (`engine/src/input`)

Actions (`Action` enum) are decoupled from physical inputs by string tokens like `"key:W"` or `"pad_axis:LeftStickY+"`, parsed in `source.rs`. `bindings.rs` merges `config.toml` overrides on top of full defaults per-action (an action listed replaces its defaults entirely; omitted actions keep defaults; `[]` unbinds). `state.rs`'s `InputState` normalizes every device to `f32` in `0.0..=1.0` and tracks sticky `just_pressed`/`just_released` edges that survive a press-and-release within a single frame — necessary because the fixed-timestep accumulator can consume zero or several ticks per real frame, and per-frame level diffing alone would miss or double-count edges.

### Errors and config

Every module exposes its own `thiserror` enum (`GpuError`, `ConfigError`, `ContourError`, `TextError`, `InputParseError`, ...); `engine::Error` aggregates the ones that can surface from `run()`. `config.rs` loads `config.toml` from the cwd, then the executable's directory (desktop only — web always uses `Config::default()`), rejects unknown keys (`deny_unknown_fields`), and lets `RUST_LOG` override the configured filter.
