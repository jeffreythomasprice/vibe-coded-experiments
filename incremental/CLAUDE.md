# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Commands

- `trunk serve` — dev server on http://localhost:8000 (needs `wasm32-unknown-unknown` target and `trunk`)
- `trunk build --release` — production build into `dist/`
- `cargo test` — runs natively (no wasm needed); all logic modules are unit-tested inline
- `cargo test <name>` — run a single test or module (e.g. `cargo test camera`)

## Architecture

Rust compiled to WASM via Trunk, drawing a hex-grid incremental game with wgpu (WebGPU, falling back to WebGL2) onto a single `<canvas id="canvas">` in `index.html`. winit owns the window and events; everything else, including all UI text and panels, is drawn on the canvas. No JS framework; `lib.rs::start` is the `#[wasm_bindgen(start)]` entry and calls `app::run`. `app`, `gpu` and their winit/wgpu/web dependencies are `wasm32`-only, so `cargo test` builds natively without them.

Layering is deliberate: only `app.rs` and `gpu/` touch winit, wgpu or `web_sys`. Everything that decides *what* to draw (`scene.rs`, `ui.rs`, `text.rs`) is pure and testable natively.

- `app.rs` — winit `ApplicationHandler` owning all state. Maps winit events to `RawInput`, creates the window from the existing canvas, and hands the async-built `Gpu` back through a user event. Redraws only while `dirty`, toasts are alive, or `Input::wants_frames()` (held keys); frame `dt` is clamped by `MAX_FRAME_DT`. Presses inside the sidebar are not forwarded to `Input`; releasing over the same sidebar icon gathers that resource.
- `input.rs` — turns `RawInput` (key/mouse/wheel/blur) into `Command`s. Bindings live in `InputConfig` (`config/input.rs`) as `(Trigger, Action)` pairs; each `Action` maps to a `Gesture` (Held/Press/Click/Drag/Wheel). The same trigger can bind several actions (left mouse is `Gather` then `Select` on click, and `DragPan` past `drag_threshold`). `Input::frame(dt)` emits commands for held keys.
- `game.rs` — game rules over `HexGrid<Cell>` (`expand`, `remove`, `gather(pos, resource)`) returning `GameError`. A cell is "owned" if it is in the grid. Each `Cell` has optional `ResourceState`s per `Resource`, a seed, and the icons generated from it; new cells get a random resource set. Knows nothing about rendering or screen coordinates.
- `hex.rs` — axial coordinates (`Axial`), `Layout` for hex↔pixel conversion, and the generic `HexGrid<T>`.
- `camera.rs` — pan/zoom with `screen_to_world` conversion; zoom clamped to `MIN_ZOOM`/`MAX_ZOOM`.
- `icon.rs` / `resource.rs` — `Resource` (five elements) and `ResourceState` (integer `Amount`s, a type alias for `u64`; each click adds `per_click` up to `max_generate`); procedural per-`Resource` icons built from vector `Layer`s in a [-1, 1] box, seeded with `rand_pcg` from the cell seed and tessellated once into a `Mesh`. `icon_slots(n)` gives the shared-footprint layout for n icons on a hex.
- `config/` — the one place for tunables: `config::theme` (`Color`, `Theme`, `PanelTheme`, `ToastTheme`, `DARK` colors/sizes), `config::input` (`InputConfig` key/mouse bindings and input tuning), `config::game` (hex size, zoom limits, resource weights/initial amounts, and other numeric constants). Change settings here rather than in the consuming modules.
- `scene.rs` — world draw data: `GridUniforms` for the procedural empty grid, `HexInstance`s for owned/selected hexes (fills, then borders, selected last), and `world_icons`. Only owned hexes are enumerated, so cost doesn't grow with zoom-out.
- `ui.rs` / `info.rs` — `info.rs` builds tooltip/sidebar content (`Summary`, `Details`, with `Icon`s) from `Game`; `ui.rs` lays that out plus toasts into `prim.rs` primitives (`Layer` of rects, icons, text) and records the sidebar rect and icon hit rects for input. Layout metrics live in `Theme::panel`/`Theme::toast`.
- `text.rs` — `Fonts` loads `assets/NotoSerif-VariableFont_wdth,wght.ttf` via `ab_glyph` (Regular and Bold through the `wght` axis); single-line measure/layout with kerning, no shaping.
- `gpu/` — wgpu renderer. One pass: `grid.wgsl` (full-screen shader that draws the empty hex grid per pixel), `hex.wgsl` (instanced owned/selected hexes), `mesh.wgsl` (icon triangles tessellated by lyon in `Icon::mesh`), `rect.wgsl` (rounded panels/bars), `text.wgsl` (glyph quads from the R8 atlas in `atlas.rs`). `batch.rs` turns primitives into instance/vertex data. Colors are written as-is to a non-sRGB surface to match the CSS look.
- `toast.rs` — pure state for transient gather toasts (`+N <icon>`): `Toasts` ages and expires them, `opacity`/`offset` give the fade and rise. `App::gather` pushes one when `Game::gather` returns a non-zero gain; `ui.rs` draws them. Colors, outline, font size, duration, fade and rise live in `Theme::toast` (`ToastTheme`).
- `error.rs` — `AppError` (browser/winit/wgpu setup failures, convertible to/from `JsValue`); `GameError` is separate and lives in `game.rs`.

### Resource assignment (temporary)

Which resources a hex gets is currently pure randomness, and is a placeholder. `Cell::random` (in `game.rs`) calls `resource::random_set`, which first rolls the *number* of resources from `COUNT_WEIGHTS` (85/10/4/0.9/0.1% for 1–5), then picks that many distinct `Resource`s uniformly with `choose_multiple`. The cell sees no neighbors, position or player choice; `Game::expand` just passes its single shared `Pcg64Mcg` (seeded in `Game::new`) into `Cell::random`. Every assigned resource starts with a default `ResourceState`, and the cell's icons are generated from the same per-cell seed.

The intended direction is for assignment to be customizable, based on neighboring hexes, or both. Keep that seam in mind: don't spread assumptions about random assignment into rendering, `info.rs`, or tests (existing tests rely only on "at least one resource" and on the weight distribution inside `resource.rs`). Changes will mainly touch `Cell::random`/`Game::expand` (which will need access to neighbors or a chosen set) and `random_set`.

Gather flow: a left click emits `Gather` then `Select`. `App::gather_at` gathers the hex's only resource, or on multi-resource hexes the resource whose icon (`icon::icon_at`) is under the cursor; clicking a sidebar icon gathers that resource on the selected hex.

Click flow: `Command::Select(screen)` → `App::select` converts via camera and layout to an `Axial`; selecting an owned hex selects it, clicking an empty hex expands from the current selection.
