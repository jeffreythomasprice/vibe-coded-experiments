# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build          # build
cargo run            # run the game (Bevy window)
cargo test           # run tests
cargo test <name>    # run a single test
```

Set `RUST_LOG=debug` for tracing output. The game uses a local Ollama model configured in `config.yaml`.

## Architecture

This is a dungeon crawl board game built with **Bevy 0.16** (ECS game engine) and **rig-core** (LLM client for procedural content generation via Ollama).

### Code Generation Pipeline

`build.rs` reads JSON schemas from `schemas/` and generates Rust structs/enums into `schema_types.rs` (via `include!` in `src/schema_types.rs`). When modifying game data structures, edit the JSON schemas — the Rust types are derived automatically.

### Game State Flow

`WaitingForSetup → StartingTurn → SelectingDestinations → Moving → RevealingRoom → Placing → (repeat)`

### Key Modules

- **`main.rs`** — Bevy app setup and ECS system registration.
- **`types.rs`** — Shared ECS components and resources (`GameState`, `Dungeon`, player components, UI markers).
- **`turn.rs`** — Turn state machine: player selection (keys 1-4), destination selection with reachable highlighting, movement animation, room reveal triggers.
- **`placement.rs`** — Room placement UI: ghost preview, rotation (R key/mouse wheel), door-matching validation.
- **`players.rs`** — Player HUD updates, position dots on grid, selection highlighting.
- **`sidebar.rs`** — Right-side info panels for rooms and players on hover.
- **`ui.rs`** — UI layout setup: HUD slots, buttons (End Turn, Reset View), status text.
- **`dungeon.rs`** — `Dungeon` struct: grid-based room placement, door matching, BFS pathfinding, candidate position logic.
- **`rendering.rs`** — Camera controls (pan/zoom), room sprites, grid drawing, path preview overlays.
- **`room_queue.rs`** — `RoomQueue` resource: async room pre-generation using tokio tasks feeding a `VecDeque` shared via `Arc<Mutex<>>`.
- **`generator/`** — LLM-powered procedural generators for rooms, items, events, effects, players, and names. Each generator sends a structured prompt to Ollama and parses JSON responses against schemas. Uses `WeightTable` for magnitude-scaled random selection.
- **`loader.rs`** — YAML/JSON file loading with JSON Schema validation.
