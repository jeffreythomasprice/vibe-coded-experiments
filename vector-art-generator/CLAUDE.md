# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run
- `bun install` — install frontend deps (uses .npmrc for default registry)
- `bun run generate` — regenerate TS types from JSON schemas
- `bun run tauri dev` — run in dev mode (auto-runs generate + vite)
- `bun run tauri build` — production build
- `cargo build` from `src-tauri/` — build Rust backend only
- `cargo test` from `src-tauri/` — run Rust tests
- `cargo clippy` from `src-tauri/` — lint Rust code

## Registry
This project uses `.npmrc` to force `registry=https://registry.npmjs.org/` because the global npmrc points to a private registry that doesn't apply here.

## Tooling
The frontend uses bun, not npm. Prefer `bunx` to `npx`.

## Architecture

Tauri v2 desktop app: Rust backend (`src-tauri/`) + vanilla TypeScript frontend (`src/`). Interactive evolutionary art — users select favorite organisms from a grid, then breed them to produce the next generation.

### IPC & Codegen
- **Source of truth**: JSON Schema files in `schemas/` (RenderedOrganism, SessionInfo, GaConfig)
- **Rust codegen**: `typify` in `src-tauri/build.rs` generates types at build time → `src-tauri/src/generated.rs` includes them via `include!`
- **TS codegen**: `scripts/generate-types.ts` (run via `bun run generate`) produces `src/generated/` — types, AJV validators, and barrel index
- When adding a new IPC type: add a `.json` schema in `schemas/`, then both Rust (automatic on cargo build) and TS (`bun run generate`) pick it up

### Genome System (Rust-only)
Genome types live entirely in `src-tauri/src/genome/` and are never exposed to the frontend. The frontend only sees rendered SVG strings via the `RenderedOrganism` IPC type.
- `types.rs` — Genome struct (shapes, colors, transforms)
- `random.rs` — random genome generation
- `render.rs` — genome → SVG string
- `crossover.rs` — two-parent recombination
- `mutation.rs` — random mutations

### GA Engine (`src-tauri/src/ga/`)
- `population.rs` — Population struct (wraps Vec<Genome>), manages generation counter and history. Held as `Mutex<Population>` in Tauri app state
- `engine.rs` — breeding logic: elitism (top 2 survive), crossover + mutation, random immigrants for diversity
- `history.rs` — tracks lineage across generations

### Frontend (`src/`)
- `main.ts` → `grid.ts` + `controls.ts` — vanilla TS, no framework
- `ipc.ts` — typed wrappers around `invoke()` for all Tauri commands
- All Tauri commands defined in `src-tauri/src/commands.rs`: `new_session`, `get_current_generation`, `breed_next_generation`, `save_session`, `load_session`, `export_svg`

### Persistence (`src-tauri/src/persistence/`)
- `session.rs` — save/load Population as JSON to disk (uses `dirs-next` for default save location)
