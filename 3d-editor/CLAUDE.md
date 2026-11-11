# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Keep the whole project formatted: run `cargo fmt --all` after changes, even if that reflows files you didn't otherwise touch.

All IPC must have strict schemas: every message on the unix socket (CLI/GUI/MCP shim to server), every MCP tool input and output, and every op's args and result. Define each as a typed Rust struct or enum that derives `Serialize`, `Deserialize` and `JsonSchema` with `#[serde(deny_unknown_fields)]`, and have clients decode into those types instead of indexing `serde_json::Value` by string. Don't build wire payloads with `json!` or use `String` for a closed set of values (use an enum). New ops need a result type in the `ops!` row and a fixture in `crates/level-cmd/tests/result_schemas.rs`; new wire messages need a sample in `crates/level-proto/tests/wire.rs`. Open `Value` fields are only acceptable where the shape truly varies, and should say so in a doc comment.

Keep documentation current as part of every change: update CLAUDE.md, README.md and the relevant files in `docs/` whenever a change affects what they say (commands, architecture, crate responsibilities, design decisions, controls). Create a new file in `docs/` when a big architectural element is added that doesn't fit an existing doc, and list it in the Docs section below. If it's not obvious whether something is a one-off or something worth remembering in the docs, ask the user.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Commands

Cargo workspace; all crates live in `crates/`.

- Build/check everything: `cargo check --workspace`
- Test all: `cargo test --workspace`; single test: `cargo test -p level-geom <test_name>`
- Lint: `cargo clippy --workspace --all-targets`
- Run a binary: `cargo run -p editor-app [level.toml]` (needs a display; joins the server's focused session, else opens `scenes/test-scene.level.toml`; also `level-cli`, `level-mcp`, `level-server`); the MCP shim is `cargo run -p level-mcp -- my.level.toml --allow-mutate`. Clients autospawn `level-server` (it must be built next to them: `cargo build --workspace`). Tests that start a server must point `LEVEL_EDITOR_CONFIG` at a temp config with its own short socket path and stop the server afterwards (see `crates/level-server/tests`).
- `level-render` tests and `level-cli screenshot` need a wgpu adapter (a GPU with Vulkan, or lavapipe) but no display. `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` select one. Check headless with `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p level-render`.

## Architecture

A TrenchBroom-style brush level editor driven equally by a human (GUI) and an AI agent (MCP/CLI), all as clients of one headless `level-server` that owns the documents. `level-core`, `level-geom`, `level-fmt`, `level-cmd` (the full Tier 0 catalog, selection, file state, undo with merging and a change feed in the session, and the schemars-backed op registry), `level-render` (offscreen readback, or into a caller's texture), `level-proto`, `level-host`, `level-server`, `level-cli` and `level-mcp` are implemented; `editor-app` is a spike-quality egui shell (four docked viewports on a shared wgpu device, navigation, picking, Select/Move/Box tools that emit commands, a live replica of the server's session); see `docs/gui.md` and `docs/server.md`.

## Docs

Design lives in `docs/`. Read the relevant doc before starting a feature, and update it when a design decision changes.

- `docs/plan.md` — overall design, architecture, tiers, risks, order of work.
- `docs/tier0-model.md` — object kinds (brush, mesh, entity, ...), IDs, hierarchy, coordinate conventions.
- `docs/tier0-commands.md` — command API: envelope, errors, registry, full command catalog, transaction semantics, CLI mapping.
- `docs/server.md` — the headless server: crates, shared config, lifecycle, wire protocol, sessions, events, concurrency.
- `docs/tier0-mcp.md` — the MCP shim: flags, tool generation, gating.
- `docs/level-format.md` — TOML level format: structure, canonical writer rules, versioning.
- `docs/gui.md` — the `editor-app` shell: architecture (replica of the server session), controls, and the egui/dock/input findings.

The central rule: the document model and command layer know nothing about UI or GPU. Dependency direction:

- `level-core` (model, stable IDs) <- `level-geom` (plane-based convex brush math) <- `level-cmd` (typed serializable commands, queries, transactions, undo, selection, `file.*`)
- `level-fmt` (versioned text level format) sits beside those; `level-cmd` uses it for `file.*`. `level-render` (wgpu, renders any camera to an offscreen texture; `Renderer::from_device` and `draw_viewport` let the GUI draw panes straight into its own textures) builds on `level-cmd` for camera resolution and the `view.screenshot` arguments and errors, and reads the document through `level-geom`.
- `level-proto` (wire types, shared TOML config, `Policy`, blocking `Client` with autospawn) is GPU-free and is all a client needs. `level-host` holds the `Hub` (sessions, per-connection policy, event fan-out, one long-lived renderer) and does no I/O; `level-server` wraps it in a unix socket. `level-mcp` (stdio shim), `level-cli` and `editor-app` (winit + egui, with a local replica of the session fed by change events) are clients; none of them depend on `level-host`. `view.screenshot` is an `Exec::Client` op: `level-cmd` never executes it, because it must stay GPU-free; the hub calls `level_render::screenshot_with_selection`. `session.*` are `Exec::Server` ops that only the hub runs.
- `level_cmd::OPS` is the single source of truth for ops (name, safety, scope, summary, args schema); `dispatch`, MCP tools and the CLI `ops`/`schema` are generated from it. Add an op by adding a row to the `ops!` table in `crates/level-cmd/src/commands/mod.rs`: name, safety, args type `=> ` result type (a struct in `commands/results.rs`, or beside the handler) `= ` handler, so args and results both have derived, strict schemas; add a fixture to `crates/level-cmd/tests/result_schemas.rs`. GUI tools emit commands and never mutate the document directly; commands take explicit object IDs, not "current selection".

Brushes are convex and stored as face planes (source of truth); vertices are derived. Meshes are polygon meshes (possibly concave, e.g. CSG results) stored as vertices and face loops.

## Dependency pinning

`egui`, `egui-wgpu` and `egui-winit` are pinned to 0.36 because `egui_dock` 0.21 requires it; bump them together. `epaint_default_fonts` (the label font for screenshots) is pinned to the same 0.36 line. egui 0.37 exists, but stay on 0.36 until `egui_dock` supports it. `schemars` is on the 1.x line `rmcp` 3.9 uses. wgpu 30 and winit 0.30 must match what egui-wgpu/egui-winit require. All versions are declared once in the root `[workspace.dependencies]`.
