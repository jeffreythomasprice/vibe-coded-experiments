# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

## Commands

Cargo workspace; all crates live in `crates/`.

- Build/check everything: `cargo check --workspace`
- Test all: `cargo test --workspace`; single test: `cargo test -p level-geom <test_name>`
- Lint: `cargo clippy --workspace --all-targets`
- Run a binary: `cargo run -p editor-app` (also `level-cli`, `level-mcp`); the MCP server is `cargo run -p level-mcp -- my.level.toml --allow-mutate`
- `level-render` tests and `level-cli screenshot` need a wgpu adapter (a GPU with Vulkan, or lavapipe) but no display. `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` select one. Check headless with `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p level-render`.

## Architecture

A TrenchBroom-style brush level editor driven equally by a human (GUI) and an AI agent (MCP/CLI). `level-core`, `level-geom`, `level-fmt`, `level-cmd` (the full Tier 0 catalog, selection and file state in the session, and the schemars-backed op registry), `level-render` (offscreen only), `level-host`, `level-cli` (one-shot commands, `ops`/`schema`, and a JSON-lines `session` mode) and `level-mcp` are implemented; `editor-app` is an empty stub.

## Docs

Design lives in `docs/`. Read the relevant doc before starting a feature, and update it when a design decision changes.

- `docs/plan.md` — overall design, architecture, tiers, risks, order of work.
- `docs/tier0-model.md` — object kinds (brush, mesh, entity, ...), IDs, hierarchy, coordinate conventions.
- `docs/tier0-commands.md` — command API: envelope, errors, registry, full command catalog, transaction semantics, CLI mapping.
- `docs/tier0-mcp.md` — the MCP server and the shared session host: flags, tool generation, gating.
- `docs/level-format.md` — TOML level format: structure, canonical writer rules, versioning.

The central rule: the document model and command layer know nothing about UI or GPU. Dependency direction:

- `level-core` (model, stable IDs) <- `level-geom` (plane-based convex brush math) <- `level-cmd` (typed serializable commands, queries, transactions, undo, selection, `file.*`)
- `level-fmt` (versioned text level format) sits beside those; `level-cmd` uses it for `file.*`. `level-render` (wgpu, renders any camera to an offscreen texture; a window target comes with the GUI) builds on `level-cmd` for camera resolution and the `view.screenshot` arguments and errors, and reads the document through `level-geom`.
- `level-host` wraps a `level-cmd` session with a permission policy and one long-lived renderer. `level-mcp`, `level-cli`'s `session` mode and (later) `editor-app` (winit + egui) are clients of it; `level-cli`'s one-shot commands use `level-cmd` directly. `view.screenshot` is an `Exec::Client` op: `level-cmd` never executes it, because it must stay GPU-free; the host calls `level_render::screenshot_with_selection`.
- `level_cmd::OPS` is the single source of truth for ops (name, safety, scope, summary, args schema); `dispatch`, MCP tools and the CLI `ops`/`schema` are generated from it. Add an op by adding a row to the `ops!` table in `crates/level-cmd/src/commands/mod.rs`. GUI tools emit commands and never mutate the document directly; commands take explicit object IDs, not "current selection".

Brushes are convex and stored as face planes (source of truth); vertices are derived. Meshes are polygon meshes (possibly concave, e.g. CSG results) stored as vertices and face loops.

## Dependency pinning

`egui`, `egui-wgpu` and `egui-winit` are pinned to 0.36 because `egui_dock` 0.21 requires it; bump them together. `epaint_default_fonts` (the label font for screenshots) is pinned to the same 0.36 line. `schemars` is on the 1.x line `rmcp` 3.9 uses. wgpu 30 and winit 0.30 must match what egui-wgpu/egui-winit require. All versions are declared once in the root `[workspace.dependencies]`.
