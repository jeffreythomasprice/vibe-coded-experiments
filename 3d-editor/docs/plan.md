# 3D Brush Level Editor — Plan

A TrenchBroom-style brush level editor in Rust, designed from day one to be driven equally by a human (GUI) and an AI agent (MCP / CLI).

Status: steps 1 to 4 of the order of work are built. Items marked **(verify)** are things I could not confirm in research and need a spike before we commit.

## 1. Goals and non-goals

**Goals**
- The classic workflow: a 4-pane editor (perspective + 3 ortho views) for building convex-brush geometry, then populating the level with entities.
- One command API underneath. The GUI, the CLI and the MCP server are all clients of it.
- Our own level format, extensible as we add features, diffable in git.
- An agent can do anything a human can, and can *see* the result.

**Non-goals (for now)**
- Compatibility with Quake `.map` / `.bsp` as the native format (import/export may come later, Tier 3).
- A runtime/game engine. The editor is the product; a minimal preview/runtime loader is a later question.
- Terrain, animation, node graphs, play-in-editor (cf. Jackdaw's scope).

## 2. Architecture

The central rule: **the document model and command layer know nothing about UI.**

```
                 +-----------------------------+
   egui GUI ---> |                             |
   CLI -------> |   Command API (typed, JSON)  | ---> Document (level model)
   MCP server -> |   queries + mutations       |        |
                 +-----------------------------+        +--> Undo history
                               |                        +--> Change events
                               v
                     Renderer (wgpu, offscreen
                     views -> screenshots + GUI)
```

Proposed crates (workspace):

| Crate | Role |
|---|---|
| `level-core` | Level model: brushes, meshes, entities, groups, layers, materials. IDs, serialization. No GPU, no UI. |
| `level-geom` | Plane-based convex brush math (clipping, vertex derivation, validity) and polygon-mesh ops (BSP CSG, welding, triangulation), picking/raycasts. |
| `level-cmd` | Command layer: every operation as a typed, serializable command; transactions; undo/redo; queries. |
| `level-render` | wgpu renderer. Renders any camera to any target (window surface or offscreen texture). Built on `level-cmd` for camera resolution and the `view.screenshot` arguments and errors; today it renders offscreen only. |
| `level-host` | Session plus renderer plus permission policy: the shared host behind the MCP server and the CLI `session` mode. |
| `level-mcp` | MCP server (tools generated 1:1 from the command registry). |
| `level-cli` | Headless CLI over the same command API: one-shot `run`/`apply`/`screenshot`, registry introspection, and a JSON-lines `session` mode. |
| `editor-app` | winit + egui + egui-wgpu shell: panels, viewports, tools, input. |
| `level-fmt` | Our file format reader/writer, versioning and migrations. |

Key design decisions:

1. **Command-first.** Tools in the GUI (clip, extrude, vertex drag) are input interpreters that *emit commands*; they never mutate the document directly. This is what makes the agent a first-class client and makes undo uniform.
2. **Explicit targets.** Commands take object IDs, not "the current selection". Selection is GUI/session state, with a separate set of selection commands for agents that want to use it.
3. **Stable IDs.** Every brush, face, entity, group and layer has a stable ID that survives save/load and undo, so an agent's references stay valid.
4. **Transactions.** A batch of commands applies atomically and undoes as one step (also how the GUI groups a drag).
5. **Headless-capable.** The model, geometry and command layers run without a window. Rendering runs offscreen with no surface and no display (verified, see risk 5).
6. **Renderer is target-agnostic.** The same code draws the four GUI viewports and agent screenshots from named cameras.
7. **One session, multiple clients.** The simplest model is the editor process owns the document and exposes the command API on a local socket; the MCP server and CLI connect to it. A headless mode runs the same process without a window. Concurrent human + agent editing means a single writer with events broadcast to all clients, no merging needed.

## 3. Tech stack

- **Language:** Rust.
- **Windowing / input:** `winit`. Note: some GUI crates have lagged on new winit versions, so pin versions that agree with the egui release **(verify)**.
- **GPU:** `wgpu`.
- **UI:** `egui` + `egui-wgpu` (+ `egui-winit`), via our own thin integration rather than `eframe`, so we control the device, the event loop and the offscreen views. `eframe` is an acceptable shortcut for the first spike.
- **Docking:** `egui_dock` or similar **(verify)** — panels for outliner, inspector, material browser, console/issues.
- **Theming:** egui looks like a debug UI by default. Options to evaluate **(verify all)**: a custom `egui::Style`/`Visuals` of our own, a themed-egui crate (e.g. catppuccin-egui or similar), and a custom icon font. I could not confirm what "egui_styled" refers to; check crates.io before choosing. Budget for owning the style ourselves.
- **Viewport embedding:** egui paint callbacks (`egui_wgpu::Callback`), or render the four views to app-owned textures on the shared wgpu device and show them as egui images. The offscreen route is preferred because it doubles as the agent screenshot path. The custom3d wgpu demo in the egui repo is the reference.
- **Math:** `glam`, `f64`. Geometry is epsilon-tolerant with snapping to a 2⁻²⁰ grid (decided in the geometry spike); see [tier0-model.md](tier0-model.md#tolerance).
- **Serialization:** `serde`; format discussed in section 5.
- **MCP:** `rmcp` over stdio, with tools generated from the command registry (`schemars` for argument schemas).
- **Pickable libs to avoid writing:** `rfd` (file dialogs), `image` (textures/screenshots).

## 4. Feature set by tier

Tier 0 = nothing works without it. Each later tier is a coherent release. Source for the TrenchBroom items is its 2026.2 manual.

### Tier 0 — Foundation + agent access

*The agent interface is Tier 0 because it dictates the command architecture. Retrofitting it later is expensive.*

Detailed specs: [tier0-model.md](tier0-model.md) (object kinds, IDs), [tier0-commands.md](tier0-commands.md) (command catalog), [tier0-mcp.md](tier0-mcp.md) (MCP server and session host), [level-format.md](level-format.md) (TOML format).

**Document and commands**
- Level model: convex brushes (face planes + material), polygon meshes (vertices + face loops, possibly concave, for CSG results and free vertex/face editing), entities (classname + key/values), stable IDs. Z-up, right-handed.
- Command layer: typed, serializable commands and queries; transactions; unlimited undo/redo; consecutive-edit merging.
- Our native level format (plain TOML, versioned, git-diffable) with load/save. See section 5.

**AI / automation (MCP + CLI)**
- MCP server exposing the command API as tools. Read-only tools available by default; mutating tools gated, with tools tagged read-only / mutating / destructive (the Godot MCP server does this and it's a good pattern).
- CLI over the same API, usable in scripts and CI (e.g. `editor-cli apply batch.json`, `editor-cli query ...`).
- Query tools: scene summary (compact: counts, bounds, layers, entity classes), list/filter objects by type/layer/tag/region, get object details, raycast, bounding-box and overlap queries.
- Mutation tools: create brush (cuboid/shape), transform (move/rotate/scale/flip), delete, clip, CSG, create/edit entity and properties, group/layer ops, selection ops.
- **Vision:** `screenshot(camera)` returning an image; named cameras (top/front/side/perspective + custom); optional overlays (grid, axes, object ID labels) so the agent can map pixels to objects.
- **Validation tools:** run the issue checker and return structured results (leaks, invalid brushes, missing targets).
- Batch/transaction tool with all-or-nothing semantics and structured errors (what failed, why, how to fix). Structured errors are what lets the agent self-correct.
- Dry-run option for any mutation.
- Session handling: the GUI process owns the document; agent and human can both edit live; GUI shows agent edits as they happen.

**Core editor loop (GUI)**
- Four-pane layout (perspective + XY, XZ, YZ), with 1/2/3-pane and maximize options.
- 3D camera: look, pan, orbit, fly keys. 2D views with linked zoom that keeps the point under the cursor fixed.
- Grid with power-of-two sizes and fractional sizes; snapping for nearly all operations.
- Selection: click, add/remove, paint-select, select all/invert, drill through occluded objects.
- Basic transforms: move, rotate, scale, flip.
- Simple shape tool: drag out a cuboid.
- Basic inspector (properties) and outliner.

### Tier 1 — A real brush editor

- **Clip tool:** keep-front / keep-back / split; snaps to faces in 3D, to the grid in 2D.
- **Vertex, edge and face tools:** move components; vertex fusing; face chopping so brushes never go concave.
- **CSG:** convex merge, subtract, hollow, intersect.
- **Entities:** point entities and brush entities; entity definitions (our own schema file, or optional FGD import) driving classnames, property types and defaults; entity browser with drag-and-drop.
- **Entity links:** visualize `target`-style references as lines in the viewport.
- **Groups** (nestable, open/close with lock semantics) and **layers** (hide, lock, omit from export).
- **Hide / isolate / lock / show all** (all undoable).
- **Materials:** material browser, per-face material assignment, face attribute editor (offset, scale, rotation).
- **Clipboard:** copy/paste (in place or at cursor), duplicate.
- **Save/load robustness:** autosave and backups.

### Tier 2 — Productivity

- **Shape tools:** stairs, arch, cylinder, cone, spheroids; complex-shape tool (convex hull from clicked points).
- **Extrude** (drag a face along its normal, with split mode) and **sweep** (arc / straight / S-bend path).
- **UV editor:** drag offset/scale/rotate, auto-fit, align/justify, UV lock during vertex/face edits, auto-alignment on new faces.
- **Linked groups:** duplicates that stay in sync, with per-property protection.
- **Command repetition** (macro-like "do that again", e.g. stairs by duplicate-move-rotate).
- **Issue browser:** live list of problems with quick fixes and click-to-select.
- **Rebindable shortcuts**, configurable mouse mapping, tablet mode.
- **Entity model display** and entity icons/gizmos.
- **Crash recovery:** crash dump of the last-good document.
- **Agent extras:** subscribe-to-changes events, named bookmarks/cameras, "explain this object" semantic descriptions, prefab/library queries.

### Tier 3 — Pipeline and compatibility

- **Export pipeline:** compile profiles (export, run tool, copy files, launch) with variables; our own runtime mesh/collision export.
- **Import/export** of Quake `.map` (Standard, Valve 220) and possibly `.bsp` read for reference; glTF export of geometry.
- **Per-project config** (the equivalent of TrenchBroom's game config): materials paths, entity defs, bounds, tags.
- **Special brush/face types** via tags (e.g. trigger, clip, sky), with filtering and shortcuts.
- **Map bounds** display and checks.
- **Leak / path visualization** if we ever build a visibility compiler.
- **Preferences UI**, updates check, localization.

### Tier 4 — Stretch

- Curved surfaces / patches (Quake 3-style), heightmap terrain.
- Lightmap or light preview, in-editor lighting approximation.
- In-editor play/preview of a minimal runtime.
- Scripting/plugin API beyond MCP (e.g. Rust or WASM extensions).
- Multi-user collaborative editing.
- Additional map formats (Quake 2/3, Hexen 2).

## 5. Level format

Plain-text TOML, `.level.toml`, single file in Tier 0, diffable and versioned. Brushes store face planes; meshes store vertices and face loops. Full spec: [level-format.md](level-format.md).

Open question: single file vs a directory of files (one per layer or group). The schema uses parent pointers on children so a later split is a pure re-partition.

## 6. AI / MCP interaction model (details)

**Principles**
- The agent works at the same level as a human: brushes, faces, entities, groups. Not raw vertices or arbitrary code.
- Everything is addressable by stable ID.
- Responses are compact and structured. Provide summary views first, drill-down on demand, to protect the context window.
- Failures are specific and machine-readable.

**Tool groups (sketch; full catalog in [tier0-commands.md](tier0-commands.md))**
- `scene.*` — summary, list (filter by kind/class/layer/group/name/region), get, bounds, raycast, query_box, overlaps
- `brush.*`, `mesh.*`, `face.*`, `csg.*` — create, clip, convert, subtract/merge/hollow/intersect, extrude, vertex moves, materials and UVs
- `entity.*` — create, set property, link, delete
- `transform.*` — move/rotate/scale/flip with explicit IDs
- `group.*`, `layer.*`
- `select.*` — optional session selection for parity with the GUI
- `history.*` — begin/commit/rollback transaction, undo, redo
- `view.*` — screenshot (named camera, size, overlays), set camera, list cameras
- `check.*` — validate and return structured issues
- `file.*` — open, save, export

**Safety**
- Read-only by default; mutating and destructive tool groups enabled explicitly (gating like the Godot MCP server).
- All mutations undoable and attributable (so a human can review an agent session as a list of transactions).

**Workflow pattern**
1. `scene.summary` → plan.
2. `begin` transaction → batch of mutations → `check.validate`.
3. `view.screenshot` from a few cameras → inspect → adjust.
4. `commit` (or rollback).

## 7. Risks and spikes

1. **Brush geometry robustness (highest risk). Spiked: plane brushes, clipping and brush CSG pass randomized tests with no failures.** Plane-based convex brushes, clipping, vertex fusing, CSG and degeneracy handling are where editors get hard. Spike: implement plane → polytope, clip, and subtract with a heavy randomized test suite before building any tools. Study how TrenchBroom (C++, open source) and Jackdaw (Rust, `jackdaw_geometry` and `jackdaw_csg` crates) do it.
2. **egui looks and feel.** Spike a themed egui with docking and four live wgpu viewports; judge it honestly before committing. Fallbacks: more custom drawing inside egui, or a different UI layer later (the command architecture keeps this swappable).
3. **egui + winit + wgpu version alignment.** These crates release in lockstep with occasional lag; pin and test.
4. **Input routing across panes.** Pointer capture, per-viewport hover/focus, and gizmo hit-testing inside docked tabs need a deliberate design **(verify with egui_dock)**.
5. **Headless/offscreen rendering** for the CLI and CI on machines without a display. **Spiked: works.** `Renderer::new` requests an adapter with no surface (retrying with `force_fallback_adapter`) and renders to an offscreen texture; the level-render tests and `level-cli screenshot` pass with `DISPLAY` and `WAYLAND_DISPLAY` unset on Vulkan (NVIDIA). Still open: a machine with no GPU. This one has no software Vulkan driver (lavapipe), and the GL backend via Mesa EGL fails device creation, so a CI runner needs lavapipe (`mesa-vulkan-drivers`) installed. `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` pick the adapter.
6. **Agent usability.** The tool surface needs real trials with an agent: too fine-grained wastes turns, too coarse is brittle. Plan to iterate on it.
7. **Mesh CSG robustness. Spiked: fixed cases and the donut pass; random slanted input fails about 0.02% of the time (chains only) and is reported as `CsgError::InvalidResult`. Exact predicates are the upgrade path if that proves too high.** Concave polygon meshes (BSP CSG, welding, snapping, n-gon triangulation, non-planar faces) are a second geometry kernel next to the plane-based one. Spike with randomized tests, e.g. donut = cylinder minus cylinder, then vertex edits and re-validation.
8. **Scope.** TrenchBroom is a decade of work. The tiers exist so we ship something useful at Tier 1 and don't need Tier 3 to be happy.

## 8. Suggested order of work

1. ~~Geometry spike (plane brushes, clip, CSG) with tests.~~ Done: `level-core` types, `level-geom` brush math and mesh CSG.
2. ~~Document + command layer + native format + undo; CLI over it.~~ Done: `level-core` document model, `level-fmt` TOML reader/writer, `level-cmd` (envelope, transactions, snapshot undo, batch, the full Tier 0 catalog, selection and files in the session, and the schemars-backed registry) and `level-cli`.
3. ~~Renderer drawing brushes to an offscreen texture; screenshots from the CLI.~~ Done: `level-geom` camera math, `level-cmd` `view.camera.*` and `view.frame`, `level-render` (flat/shaded/wireframe, grid, axes, bounds, id labels, highlight) and `level-cli screenshot`. `view.screenshot` is client-side because `level-cmd` stays GPU-free; `level-host` calls `level_render::screenshot` the same way. The `selection` overlay arrived with selection state. Labels are occlusion-aware (an object id buffer), with hide, clip-height, colour modes, a legend and a `path` to save the PNG.
4. ~~MCP server on the command API (now an agent can build and view a level headlessly).~~ Done: `level-host` (session, one long-lived renderer, gating), `level-mcp` (tools generated from `OPS`, image content for `view.screenshot`, read/mutate/destructive flags) and the CLI `session` mode. Args schemas only; result schemas and event push come later. See [tier0-mcp.md](tier0-mcp.md).
5. egui shell with four viewports, camera, selection, grid, move/cuboid tool.
6. Tier 1 tools one at a time, each landing as commands first, then GUI.

Building the headless path (steps 2–4) before the GUI keeps the architecture honest: if an agent can build a level without the GUI, the GUI is just another client.

## 9. Reference material

- TrenchBroom 2026.2 manual (feature inventory): https://trenchbroom.github.io/manual/latest
- TrenchBroom source (C++): https://github.com/TrenchBroom/TrenchBroom
- Jackdaw (Bevy brush editor, Rust): https://github.com/jbuehler23/jackdaw
- egui custom wgpu 3D demo: https://github.com/emilk/egui/blob/041f2e64bac778c9095fbf4316dc1f7c7bceb670/crates/egui_demo_app/src/apps/custom3d_wgpu.rs
- Godot MCP server (tool gating pattern): https://github.com/hybridindie/godot-mcp
- Unity MCP servers (scene summary tools): https://github.com/usmanbutt-dev/unity-mcp
- Unreal MCP bridge (editor-hosted endpoint): https://github.com/ChiR24/Unreal_mcp/wiki/Home
- Godot TSCN format (diffable text scenes): https://docs.godotengine.org/en/stable/engine_details/file_formats/tscn.html
- Rust GUI survey 2026: https://blog.wybxc.cc/blog/rust-gui-survey-2026/
