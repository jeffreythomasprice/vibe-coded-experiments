# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Keep the whole project formatted: run `cargo fmt --all` after changes, even if that reflows files you didn't otherwise touch.

Types are schema-first: the committed JSON Schema files in `schemas/` are the source of truth, and the Rust types are generated from them. Never write or edit one of these types by hand: edit the schema and rebuild. This covers component definitions (`schemas/component-defs.schema.json`) and built-in component data (`schemas/components.schema.json`) and, as they are migrated (see `docs/schemas.md` for what has moved), IPC (wire messages, op args and results, MCP tool schemas) and the TOML scene format. A new item in any of these categories starts as a definition in `schemas/` and uses only the generated type; domain methods go in `impl` blocks beside it, never in a copy of the struct. The pipeline: a crate's `build.rs` calls `editor_schema_gen::generate` (typify) with the definition names it owns, and writes the types into `OUT_DIR`; every definition name is unique across all files, `$ref`s between files are bundled by name, and a generated type implements `editor_schema::Schema`. All untrusted input (files, socket messages, op args) must go through `editor_schema::parse`, `parse_str` or `parse_slice`, which validate against the schema and then build the type, so errors carry JSON pointers; do not call `serde_json::from_*` on these types directly. Schemas must be closed (`additionalProperties: false`) unless the shape truly varies, and then say so in the description. Typify limits to design around: `const` is lost (use a one-value `enum`), a tagged union of `$ref` records needs the tag as a one-value `enum` property inside each record, and struct fields come out alphabetically.

Until an area is migrated, its types keep the old convention. For IPC that is a typed struct or enum deriving `Serialize`, `Deserialize` and `JsonSchema` with `#[serde(deny_unknown_fields)]`; clients decode into those types instead of indexing `serde_json::Value` by string; no `json!` wire payloads and no `String` for a closed set of values. New ops need a result type in the `ops!` row and a fixture in `crates/editor-cmd/tests/result_schemas.rs`; new wire messages need a sample in `crates/editor-proto/tests/wire.rs`. Open `Value` fields are only acceptable where the shape truly varies, and should say so in a doc comment.

Components: adding or changing a component (built-in or in `scenes/defs`) updates `docs/components.md` in the same change. Built-in component data types are schema-first in `schemas/components.schema.json`; a new built-in also needs a row in `BUILTINS` (`crates/editor-cmd/src/component_defs.rs`), parsing in `Sim::load` and a system if it has behaviour (see `docs/ecs.md`).

Keep documentation current as part of every change: update CLAUDE.md, README.md and the relevant files in `docs/` whenever a change affects what they say (commands, architecture, crate responsibilities, design decisions, controls). Create a new file in `docs/` when a big architectural element is added that doesn't fit an existing doc, and list it in the Docs section below. If it's not obvious whether something is a one-off or something worth remembering in the docs, ask the user.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Commands

Cargo workspace; all crates live in `crates/`.

- Build/check everything: `cargo check --workspace`
- Test all: `cargo test --workspace`; single test: `cargo test -p editor-geom <test_name>`
- Lint: `cargo clippy --workspace --all-targets`
- Run a binary: `cargo run -p editor-app [scene.toml]` (needs a display; joins the server's focused session, else opens `scenes/test.scene.toml`; also `editor-game` (the first-person play client, same rules: `cargo run -p editor-game [scene.toml | --session <id>]`), `editor-cli`, `editor-mcp`, `editor-server`); the MCP shim is `cargo run -p editor-mcp -- my.scene.toml --allow-mutate`. Clients autospawn `editor-server` (it must be built next to them: `cargo build --workspace`). Tests that start a server must point `EDITOR_CONFIG` at a temp config with its own short socket path and its own `[recovery] dir` (the server autosaves unsaved sessions there) and stop the server afterwards (see `crates/editor-server/tests`).
- `editor-render` tests and `editor-cli screenshot` need a wgpu adapter (a GPU with Vulkan, or lavapipe) but no display. `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` select one. Check headless with `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p editor-render`.

## Architecture

A TrenchBroom-style brush scene editor driven equally by a human (GUI) and an AI agent (MCP/CLI), all as clients of one headless `editor-server` that owns the documents. `editor-core`, `editor-geom` (including UV maths and texture lock), `editor-fmt`, `editor-cmd` (the full Tier 0 catalog, materials and UV ops, selection and face selection, the component definitions catalog (built-in components plus definitions files; JSON Schema validation via `jsonschema`) and entity link resolution, file state, undo with merging and a change feed in the session, and the schemars-backed op registry), `editor-sim` (the GPU-free ECS and simulation), `editor-render` (offscreen readback, or into a caller's texture; draws material images; can pose entity geometry), `editor-proto` (including the `Replica`), `editor-host`, `editor-server`, `editor-cli`, `editor-mcp` and `editor-game` (the play client) are implemented; `editor-app` is a spike-quality egui shell (four docked viewports on a shared wgpu device, navigation, picking, Select/Move/Box/Vertex/Edge/Face/Clip/Surface tools that emit commands, face and material inspectors, a live replica of the server's session, a per-component JSON editor, a component browser with drag-and-drop and link arrows, and a Play button that starts `editor-game`, clipboard shortcuts and a recovery banner); the server also owns the shared clipboard and the autosave/backup/recovery directory; see `docs/gui.md`, `docs/entities.md`, `docs/ecs.md`, `docs/game.md` and `docs/server.md`.

## Docs

Design lives in `docs/`. Read the relevant doc before starting a feature, and update it when a design decision changes.

- `docs/plan.md` — overall design, architecture, tiers, risks, order of work.
- `docs/tier0-model.md` — object kinds (brush, mesh, entity, ...), IDs, hierarchy, coordinate conventions.
- `docs/tier0-commands.md` — command API: envelope, errors, registry, full command catalog, transaction semantics, CLI mapping.
- `docs/server.md` — the headless server: crates, shared config, autosave and backups, lifecycle, wire protocol, sessions, events, concurrency.
- `docs/tier0-mcp.md` — the MCP shim: flags, tool generation, gating.
- `docs/schemas.md` — schema-first types: `schemas/` layout, the codegen pipeline (`editor-schema-gen`), the runtime (`editor-schema`), typify limits, and which areas are migrated.
- `docs/entities.md` — entities as a pose plus components (open JSON), component definitions with JSON Schema, `editor.linkType` links, validation policy and the GUI pieces.
- `docs/ecs.md` — the `editor-sim` ECS and simulation: how a document becomes a world, systems, time, how to add a component or system.
- `docs/components.md` — the implied transform and every component (built-in and shipped), with exact semantics. Update it whenever a component is added or changed.
- `docs/game.md` — the `editor-game` client: running, controls, rendering style, `simulate` screenshots, limits.
- `docs/scene-format.md` — TOML scene format: structure, canonical writer rules, versioning.
- `docs/materials.md` — materials, textures (absolute, relative, embedded), UV model, texture lock, alignment ops, face selection, rendering.
- `docs/gui.md` — the `editor-app` shell: architecture (replica of the server session), controls, and the egui/dock/input findings.

The central rule: the document model and command layer know nothing about UI or GPU. Dependency direction:

- `editor-core` (model, stable IDs) <- `editor-geom` (plane-based convex brush math) <- `editor-cmd` (typed serializable commands, queries, transactions, undo, selection, `file.*`)
- `editor-sim` (the ECS: a `World` of components and resources, systems, `Sim::load(&Document)`; entities become ECS entities and world brushes and meshes become static geometry; GPU-free, depends on `editor-core`, `editor-geom` and `editor-schema`) is what the game client runs; the editor never runs it. `editor-schema` (committed schemas bundled by definition name, `parse`/`parse_str`/`parse_slice`, `CompiledSchema` for user-authored schemas) is GPU-free and has no editor dependencies; `editor-schema-gen` is a build-dependency that turns those schemas into Rust types. `editor-core` depends on both.
- `editor-fmt` (versioned text scene format) sits beside those; `editor-cmd` uses it for `file.*`. `editor-render` (wgpu, renders any camera to an offscreen texture; `Renderer::from_device` and `draw_viewport` let the GUI draw panes straight into its own textures) builds on `editor-cmd` for camera resolution and the `view.screenshot` arguments and errors, and reads the document through `editor-geom`.
- `editor-proto` (wire types, shared TOML config, `Policy`, blocking `Client` with autospawn, and `Replica`, the local copy of a session) is GPU-free and is all a client needs. `editor-host` holds the `Hub` (sessions, per-connection policy, event fan-out, one long-lived renderer) and does no I/O; `editor-server` wraps it in a unix socket. `editor-mcp` (stdio shim), `editor-cli`, `editor-app` (winit + egui, with a `Replica` fed by change events) and `editor-game` (winit + wgpu, no egui: one first-person camera, noclip controls, the same `Replica`) are clients; none of them depend on `editor-host`. `view.screenshot` is an `Exec::Client` op (with `simulate` it renders as the game client does, through `editor-sim`): `editor-cmd` never executes it, because it must stay GPU-free; the hub calls `editor_render::screenshot_with_selection`. `session.*` are `Exec::Server` ops that only the hub runs.
- `editor_cmd::OPS` is the single source of truth for ops (name, safety, scope, summary, args schema); `dispatch`, MCP tools and the CLI `ops`/`schema` are generated from it. Add an op by adding a row to the `ops!` table in `crates/editor-cmd/src/commands/mod.rs`: name, safety, args type `=> ` result type (a struct in `commands/results.rs`, or beside the handler) `= ` handler, so args and results both have derived, strict schemas; add a fixture to `crates/editor-cmd/tests/result_schemas.rs`. GUI tools emit commands and never mutate the document directly; commands take explicit object IDs, not "current selection". Locked objects are protected once, in `editor_cmd::dispatch` (`commands/locks.rs`), by comparing what an op journaled; handlers do not check locks themselves.

Brushes are convex and stored as face planes (source of truth); vertices are derived. Meshes are polygon meshes (possibly concave, e.g. CSG results) stored as vertices and face loops.

## Dependency pinning

`egui`, `egui-wgpu` and `egui-winit` are pinned to 0.36 because `egui_dock` 0.21 requires it; bump them together. `epaint_default_fonts` (the label font for screenshots) is pinned to the same 0.36 line. egui 0.37 exists, but stay on 0.36 until `egui_dock` supports it. `schemars` is on the 1.x line `rmcp` 3.9 uses, except in `editor-schema-gen`, which needs 0.8 because `typify` 0.6 does. Both can coexist; `typify` is only a build-dependency. wgpu 30 and winit 0.30 must match what egui-wgpu/egui-winit require. All versions are declared once in the root `[workspace.dependencies]`.
