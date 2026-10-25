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

Rust compiled to WASM via Trunk, drawing a hex-grid incremental game onto a single `<canvas id="canvas">` in `index.html`. No JS framework; `lib.rs::start` is the `#[wasm_bindgen(start)]` entry and calls `App::start`.

Layering is deliberate: only `app.rs`, `canvas.rs`/`render.rs` and `overlay.rs` touch `web_sys`. Everything else is pure and testable natively.

- `app.rs` — owns all state in `Rc<RefCell<App>>`, registers DOM listeners (closures are `forget()`-ed), and runs the `requestAnimationFrame` loop. Rendering is dirty-flag driven: `apply(Command)` sets `dirty`, and `frame` only redraws when it is set. Frame `dt` is clamped by `MAX_FRAME_DT`.
- `input.rs` — turns `RawInput` (key/mouse/wheel/blur) into `Command`s. Bindings live in `InputConfig` as `(Trigger, Action)` pairs; each `Action` maps to a `Gesture` (Held/Press/Click/Drag/Wheel). The same trigger can bind several actions (left mouse is `Select` on click, `DragPan` past `drag_threshold`, and `Gather` every frame while held without dragging). `Input::frame(dt)` emits commands for held keys.
- `game.rs` — game rules over `HexGrid<Cell>` (`expand`, `remove`, `gather`) returning `GameError`. A cell is "owned" if it is in the grid. Each `Cell` has optional `ResourceState`s per `Resource`, a seed, and the icons generated from it; new cells get a random resource set. Knows nothing about rendering or screen coordinates.
- `hex.rs` — axial coordinates (`Axial`), `Layout` for hex↔pixel conversion, and the generic `HexGrid<T>`.
- `camera.rs` — pan/zoom with `screen_to_world` conversion; zoom clamped to `MIN_ZOOM`/`MAX_ZOOM`.
- `icon.rs` / `resource.rs` — `Resource` (five elements) and `ResourceState`; procedural per-`Resource` icons built from vector `Layer`s in a [-1, 1] box, seeded with `rand_pcg` from the cell seed. `icon_slots(n)` gives the shared-footprint layout for n icons on a hex.
- `render.rs` / `theme.rs` — stateless canvas drawing of grid, selection and hex icons using a `Theme`.
- `overlay.rs` / `info.rs` — DOM tooltip (`#tooltip`, hovered owned hex) and sidebar (`#sidebar`, selected hex) in `index.html`, themed via `Theme`; `info.rs` builds their content (icon SVGs plus resource values) from `Game`; `Overlay` rebuilds the DOM only when that content changes.
- `error.rs` — `AppError` (browser/setup failures, convertible to/from `JsValue`); `GameError` is separate and lives in `game.rs`.

### Resource assignment (temporary)

Which resources a hex gets is currently pure randomness, and is a placeholder. `Cell::random` (in `game.rs`) calls `resource::random_set`, which first rolls the *number* of resources from `COUNT_WEIGHTS` (85/10/4/0.9/0.1% for 1–5), then picks that many distinct `Resource`s uniformly with `choose_multiple`. The cell sees no neighbors, position or player choice; `Game::expand` just passes its single shared `Pcg64Mcg` (seeded in `Game::new`) into `Cell::random`. Every assigned resource starts with a default `ResourceState`, and the cell's icons are generated from the same per-cell seed.

The intended direction is for assignment to be customizable, based on neighboring hexes, or both. Keep that seam in mind: don't spread assumptions about random assignment into rendering, `info.rs`, or tests (existing tests rely only on "at least one resource" and on the weight distribution inside `resource.rs`). Changes will mainly touch `Cell::random`/`Game::expand` (which will need access to neighbors or a chosen set) and `random_set`.

Click flow: `Command::Select(screen)` → `App::select` converts via camera and layout to an `Axial`; selecting an owned hex selects it, clicking an empty hex expands from the current selection.
