# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Commands

```bash
trunk serve                                  # dev server on port 8000
trunk build                                  # output in dist/

cargo test                                   # runs natively, no browser needed
cargo test hound::tests::fire_can_be_put_out # a single test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-unknown-unknown -- -D warnings

# Image-to-stamp converter: a separate native crate with its own workspace
cargo build --release --manifest-path tools/stampify/Cargo.toml
```

## Architecture

**Art is data until the last step.** Parts are drawn into a `PixBuf` of `(Material, shade 0-3)` pixels, never RGB. `PixBuf::paint` resolves colours through a `Palette` into the RGBA frame, so recolouring costs nothing.

**Parts are either shapes or stamps.** Code draws the masses that move or vary in size (torso, legs, hands, tail). Text bitmaps in `assets/stamps/` hold detail where single pixels matter (faces, hair, hats, the hound's head, flames). Stamps are compiled in with `include_str!` and parsed by `src/stamp.rs`, which also documents the format; shade characters `1`-`4` map to shades 0-3.

**Render pipeline** (`src/human.rs`, `src/hound.rs`): the animation time gives a `Pose` (pure function of `(Anim, t)`); each `Part` gets a draw order from `Facing::project`, which turns body-relative `(right, forward)` into `(screen x, depth)`; each part is drawn on its own full-sprite layer, shaded and outlined there, then composited back to front. Only side views facing right are authored; west mirrors them.

**Adding or changing a creature kind** touches `src/creature.rs` (the `Kind`/`Creature` dispatch, `Kind::ALL`, and per-kind `origin`, `reach` and `shadow`), the kind's own module, `src/stamp.rs` and `src/palette.rs`. `reach` bounds movement on screen and must cover the rendered sprite; `every_kind_stays_inside_its_sprite_and_reach` enforces this.

**Seeds fully determine a creature** (`src/rng.rs`). Each `rng.pick` shifts every later one, so add new random choices last to keep existing seeds looking the same.

**Host loop** (`src/main.rs`, `src/canvas.rs`): each animation frame writes the scene to a low-resolution offscreen canvas, then scales it by an integer factor onto the window-sized canvas. Text uses the hand-made font in `src/font.rs`, which only has digits and a few capital letters; add glyphs before drawing new strings, as unknown characters render blank.

## Art workflow

Use the `pixel-sprites` skill for new or changed art and creature kinds. The flow is: image model sheets (`art/raw/`) → `stampify` → `art/converted/` → hand cleanup → `assets/stamps/`. Never convert straight into `assets/stamps/`, which holds hand-edited files. Tests cannot judge the art, so check visual changes in the browser across every facing, walk frame and several seeds.
