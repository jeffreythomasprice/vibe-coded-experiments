# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

RoboWar is a robot arena combat simulator in Rust. Robots are programmed in a custom assembly language (BotASM), executed by a VM, and fight in a 2D physics arena.

## Commands

- Build: `cargo build`
- Run tests: `cargo test`
- Run a single test: `cargo test test_name`
- Run tests in a module: `cargo test vm::assembler`
- Check without building: `cargo check`
- Lint: `cargo clippy`
- Run a match (project config): `cargo run -p robowar-cli -- run --config examples/robowar.toml`
- Run a match (explicit args): `cargo run -p robowar-cli -- run --arena examples/arenas/square.toml examples/robots/spinner.toml examples/robots/patrol.toml`
- Assemble a program: `cargo run -p robowar-cli -- assemble examples/robots/spinner.asm`
- Robot info: `cargo run -p robowar-cli -- info examples/robots/spinner.toml`
- Visualize a match: `cargo run -p robowar-visualizer -- --config examples/robowar.toml`
- Visualize (explicit args): `cargo run -p robowar-visualizer -- --arena examples/arenas/square.toml examples/robots/spinner.toml examples/robots/patrol.toml`

## Architecture

The project is a Cargo workspace with three crates:
- `crates/shared` (`robowar-shared`) — library crate with VM, simulation, and config modules.
- `crates/cli` (`robowar-cli`) — binary crate with the CLI.
- `crates/visualizer` (`robowar-visualizer`) — Bevy-based match visualizer (binary: `robowar-viz`).

### VM (`crates/shared/src/vm/`)
- `instruction.rs` — `Instruction` enum (~40 opcodes), `Operand` enum, `RegisterId` enum, `Program` struct.
- `assembler.rs` — two-pass assembler: text `.asm` → `Program` IR. Labels resolve to instruction indices. Pseudo-instructions (INC, DEC, MOVI) expand during assembly.
- `registers.rs` — `RegisterFile` with 16 GP registers (R0-R15), read/write special-purpose registers (Sp, Spd, Trn, Trt), and read-only registers (Hp, X, Y, Hdg, etc.) updated by simulation. `Flags` struct for compare operations. Values stored as `u32`, reinterpreted via `read_f32`/`write_f32`/`read_i32`/`write_i32`.
- `memory.rs` — 4096-cell linear memory with bounds-checked `read()`/`write()`.
- `executor.rs` — `VmState` struct (program, PC, registers, memory). `execute_tick()` runs instructions up to a cycle budget. `Action` enum (Scan, Fire) returned to simulation. Cost model: Scan=5, Fire=3, others=1.

### Simulation (`crates/shared/src/sim/`)
- `match_runner.rs` — `run_match()` and `build_match_state()` entry points. `MatchConfig` (format, max_ticks, arena, robots, constants, seed). `MatchResult` with per-robot `RobotStats`.
- `tick.rs` — `MatchState` and 7-phase tick pipeline: program execution → turret rotation → movement → projectile movement → collision resolution → damage → win check. Updates read-only registers from robot state.
- `robot.rs` — `Robot` struct (position, heading, turret, speed, HP, loadout, vm). `Loadout` with point allocation for hp/speed/armor/gun. Derived stats: `max_hp()`, `max_speed()`, `armor()`, `gun_power()`.
- `physics.rs` — Rapier2D integration: `PhysicsWorld` with rigid bodies, colliders, ray casting for scan, physics stepping, position sync.
- `arena.rs` — `Arena` with bounds, static `ArenaBody` obstacles (Rectangle, Circle), spawn points. `empty_arena()` creates default 800x600 walled arena.
- `projectile.rs` — `Projectile` with position, direction, speed, damage, lifetime tracking.
- `damage.rs` — Damage calculations: projectile (armor reduction, min 1), wall collision (speed-based), robot collision (closing speed).
- `observer.rs` — `MatchObserver` trait with callbacks for tick events, damage, destruction, match end.
- `replay.rs` — `MatchReplay` recording: `TickSnapshot` with robot/projectile snapshots and `TickEvent`s (scan rays, damage, destruction).

### Config (`crates/shared/src/config/`)
- `constants.rs` — `SimConstants`: tick rate (60 Hz), cycle budget (100), arena size (800x600), point budget (20), base stats, damage factors, rotation speeds.
- `loadout.rs` — `RobotConfig`/`LoadoutConfig` deserialized from TOML. Point validation against budget.
- `arena_config.rs` — `ArenaConfig` from TOML with body/spawn-point validation.
- `cli_args.rs` — `MatchArgs` struct (clap): robot paths, `--arena`, `--ticks`, `--seed`, `--config` (default: `robowar.toml`). `LogArgs` struct: `--log-dir`, `--no-file-log`. Both shared between CLI and visualizer.
- `logging.rs` — `init_logging()`: configures `tracing-subscriber` with stdout + optional file logging. Log dir from CLI arg, `LoggingConfig`, or platform data dir.
- `project.rs` — `ProjectConfig` loaded from `robowar.toml`: arena, robots, max_ticks, seed, groups (with `@group` references and glob expansion), `VisualizerConfig` (speed, `KeybindingsConfig`).
- `resolve.rs` — `resolve_match_config()`: merges CLI args + project config, resolves `@group` references (random selection via seed), expands globs in groups, loads/validates robots, assembles programs, builds `MatchConfig`.

### CLI (`crates/cli/src/main.rs`)
Clap-based CLI with three commands: `run` (execute a match), `assemble` (compile .asm), `info` (show robot stats). The `run` command loads a `ProjectConfig` and resolves it with `MatchArgs`.

### Visualizer (`crates/visualizer/src/`)
- `main.rs` — Clap CLI using shared `MatchArgs` plus `--speed`. Loads `ProjectConfig`, resolves match config, converts `KeybindingsConfig` into `Keybindings` resource, launches Bevy app with `SimulationPlugin`.
- `simulation.rs` — `SimulationPlugin` with `SimulationState` (wraps `MatchState`), `SimConfig` (constants, speed, paused), `TickTimer` (repeating timer for tick-driven stepping). Startup system builds match state from `MatchConfig`; update systems step simulation and check for match end.
- `arena.rs` — Startup systems `spawn_camera` (2D camera centered on arena) and `spawn_arena` (background quad, wall/obstacle sprites from `ArenaBody`). `ArenaEntity` marker component. `CameraState` resource for zoom/pan bounds.
- `robot.rs` — `RobotMarker`/`TurretMarker` components. Startup system `spawn_robots` creates circle sprites with turret barrel children and name labels. Update system `sync_robots` syncs positions, rotations, and visibility from `SimulationState`.
- `projectile.rs` — `ProjectileMarker` component. Update system `sync_projectiles` spawns/despawns/syncs projectile sprites against simulation state.
- `hud.rs` — UI overlay with tick counter, match status, and per-robot HP bars (color-coded green→yellow→red). Startup system `setup_hud` builds UI node tree; update system `update_hud` syncs from simulation.
- `controls.rs` — Keyboard/mouse input system using configurable `Keybindings`. Controls: pause, speed +/-, restart, menu toggle, zoom (keyboard + mouse wheel), pan (keyboard + mouse drag). `MatchSetup` resource for restart.
- `keybindings.rs` — `Keybindings` resource (Vec<KeyCode> per action). Converts from `KeybindingsConfig` strings. `parse_key()` maps string names to `KeyCode`. `all_bindings()` for controls overlay display.
- `menu.rs` — In-game menu overlay with Controls and Quit buttons. `MenuState` resource (open, confirm_quit, controls_open). Quit confirmation dialog. Controls overlay shows all keybindings.
- `effects.rs` — Visual effects via state-diff detection. `VisualEvent` enum (DamageFlash, MuzzleFlash, Destruction). Pre/post tick comparison detects HP changes, deaths, and new projectiles. Transient effect entities with `EffectLifetime` component auto-fade and despawn.

### Examples (`examples/`)
- Four example robots with `.asm` programs and `.toml` configs: dumb, spinner, patrol, wanderer.
- Two arenas: `arenas/square.toml`, `arenas/wide.toml`.
- Project config files: `robowar.toml` (basic match), `robowar-square.toml` (4-robot FFA using `@group` with glob).

## Conventions

- Use `anyhow::Result` unless a specific error type exists.
- Use `anyhow!` / `bail!` to add context to errors, include line numbers for assembler errors.
- Avoid unneeded comments. Only comment really complicated code.
- Registers are stored as `u32` and reinterpreted as `i32` or `f32` via bit-casting.
- Read-only registers can only be written via `write_readonly()` (used by the simulation layer).
