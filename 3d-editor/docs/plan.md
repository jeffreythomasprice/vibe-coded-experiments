# 3D Brush Editor — Plan

A TrenchBroom-style brush scene editor in Rust, designed from day one to be driven equally by a human (GUI) and an AI agent (MCP / CLI).

Status: steps 1 to 4 of the order of work are built, the GUI spikes (7.2 to 7.4) are done (see [gui.md](gui.md)), and the editor is a headless server with the CLI, MCP and GUI as its clients (see [server.md](server.md)). Items marked **(verify)** are things I could not confirm in research and need a spike before we commit.

## 1. Goals and non-goals

**Goals**
- The classic workflow: a 4-pane editor (perspective + 3 ortho views) for building convex-brush geometry, then populating the scene with entities.
- One command API underneath. The GUI, the CLI and the MCP server are all clients of it.
- Our own scene format, extensible as we add features, diffable in git.
- An agent can do anything a human can, and can *see* the result.

**Non-goals (for now)**
- Compatibility with Quake `.map` / `.bsp` as the native format (import/export may come later, Tier 3).
- A full runtime/game engine. The editor is the product. A deliberately small simulation exists (a hand-written ECS, `keyframe_animation`, and a noclip first-person client) to play scenes: see `docs/ecs.md`, `docs/components.md` and `docs/game.md`.
- Terrain, node graphs (cf. Jackdaw's scope).

## 2. Architecture

The central rule: **the document model and command layer know nothing about UI.**

```
                      unix socket                 editor-server
   egui GUI ---> |                        +-----------------------------+
   CLI -------> |  JSON lines (editor-proto) |   Command API (typed, JSON)  | ---> Sessions: Document (scene model)
   MCP shim --> |                        |   queries + mutations       |        |
                 |                        +-----------------------------+        +--> Undo history
                 |                                      |                         +--> Change events (to all clients)
                 |                                      v
                 +--- events <------------   Renderer (wgpu, offscreen
                                              views -> screenshots)
```

Proposed crates (workspace):

| Crate | Role |
|---|---|
| `editor-core` | Scene model: brushes, meshes, entities, groups, layers, materials. IDs, serialization. No GPU, no UI. |
| `editor-geom` | Plane-based convex brush math (clipping, vertex derivation, validity) and polygon-mesh ops (BSP CSG, welding, triangulation), picking/raycasts. |
| `editor-cmd` | Command layer: every operation as a typed, serializable command; transactions; undo/redo; queries. |
| `editor-render` | wgpu renderer. Renders any camera to any target (window surface or offscreen texture). Built on `editor-cmd` for camera resolution and the `view.screenshot` arguments and errors. Draws offscreen with readback (screenshots, picking) or straight into a caller's texture on a shared device (the GUI panes). |
| `editor-proto` | GPU-free client side of the server: wire types, the shared TOML `Config`, `Policy`, and a blocking `Client` that autospawns the server. |
| `editor-host` | `Hub`: the sessions, connections with per-connection policy, the focused session, change-event fan-out and the one shared renderer. No I/O. |
| `editor-server` | The headless server binary: a unix socket around one `Hub`. Owns every open document. See [server.md](server.md). |
| `editor-mcp` | MCP server over stdio (tools generated 1:1 from the command registry); a thin client of `editor-server`. |
| `editor-cli` | CLI client: `run`/`apply`/`screenshot`, `session` (JSON lines, optional events), `watch`, `sessions`, `server`, and registry introspection. |
| `editor-app` | winit + egui + egui-wgpu shell: panels, viewports, tools, input. A client of `editor-server` keeping a live replica of the document; see [gui.md](gui.md). |
| `editor-fmt` | Our file format reader/writer, versioning and migrations. |

Key design decisions:

1. **Command-first.** Tools in the GUI (clip, extrude, vertex drag) are input interpreters that *emit commands*; they never mutate the document directly. This is what makes the agent a first-class client and makes undo uniform.
2. **Explicit targets.** Commands take object IDs, not "the current selection". Selection is GUI/session state, with a separate set of selection commands for agents that want to use it.
3. **Stable IDs.** Every brush, face, entity, group and layer has a stable ID that survives save/load and undo, so an agent's references stay valid.
4. **Transactions.** A batch of commands applies atomically and undoes as one step (also how the GUI groups a drag).
5. **Headless-capable.** The model, geometry and command layers run without a window. Rendering runs offscreen with no surface and no display (verified, see risk 5).
6. **Renderer is target-agnostic.** The same code draws the four GUI viewports and agent screenshots from named cameras.
7. **One server, multiple clients.** A headless `editor-server` owns the documents (sessions) and exposes the command API on a unix socket; the GUI, the CLI and the MCP shim are all clients of it, found through one shared config file. Concurrent human + agent editing means a single writer with every change broadcast to all clients attached to the session, no merging needed. Transactions are exclusive to the connection that opened them. See [server.md](server.md).

## 3. Tech stack

- **Language:** Rust.
- **Windowing / input:** `winit`. Pinned to 0.30 to agree with egui-winit 0.36.
- **GPU:** `wgpu`.
- **UI:** `egui` + `egui-wgpu` (+ `egui-winit`), via our own thin integration rather than `eframe`, so we control the device, the event loop and the offscreen views. `eframe` is an acceptable shortcut for the first spike.
- **Docking:** `egui_dock` (verified in the spike) — panels for outliner, inspector, material browser, console/issues.
- **Theming:** egui looks like a debug UI by default. We own the style (`theme.rs`); catppuccin-egui does not support egui 0.36. `egui_styled` (0.9.0, Tailwind-style utilities) and an icon font are untried.
- **Viewport embedding:** egui paint callbacks (`egui_wgpu::Callback`), or render the four views to app-owned textures on the shared wgpu device and show them as egui images. The offscreen route is preferred because it doubles as the agent screenshot path. The custom3d wgpu demo in the egui repo is the reference.
- **Math:** `glam`, `f64`. Geometry is epsilon-tolerant with snapping to a 2⁻²⁰ grid (decided in the geometry spike); see [tier0-model.md](tier0-model.md#tolerance).
- **Serialization:** `serde`; format discussed in section 5.
- **MCP:** `rmcp` over stdio, with tools generated from the command registry (`schemars` for argument schemas).
- **Pickable libs to avoid writing:** `rfd` (file dialogs), `image` (textures/screenshots).

## 4. Feature set by tier

Tier 0 = nothing works without it. Each later tier is a coherent release. Source for the TrenchBroom items is its 2026.2 manual.

### Tier 0 — Foundation + agent access

*The agent interface is Tier 0 because it dictates the command architecture. Retrofitting it later is expensive.*

Detailed specs: [tier0-model.md](tier0-model.md) (object kinds, IDs), [tier0-commands.md](tier0-commands.md) (command catalog), [server.md](server.md) (headless server, protocol, sessions), [tier0-mcp.md](tier0-mcp.md) (MCP shim), [scene-format.md](scene-format.md) (TOML format).

**Document and commands**
- Scene model: convex brushes (face planes + material), polygon meshes (vertices + face loops, possibly concave, for CSG results and free vertex/face editing), entities (a pose plus named components), stable IDs. Z-up, right-handed.
- Command layer: typed, serializable commands and queries; transactions; unlimited undo/redo; consecutive-edit merging.
- Our native scene format (plain TOML, versioned, git-diffable) with load/save. See section 5.

**AI / automation (MCP + CLI)**
- MCP server exposing the command API as tools. Read-only tools available by default; mutating tools gated, with tools tagged read-only / mutating / destructive (the Godot MCP server does this and it's a good pattern).
- CLI over the same API, usable in scripts and CI (e.g. `editor-cli apply batch.json`, `editor-cli query ...`).
- Query tools: scene summary (compact: counts, bounds, layers, entity classes), list/filter objects by type/layer/tag/region, get object details, raycast, bounding-box and overlap queries.
- Mutation tools: create brush (cuboid/shape), transform (move/rotate/scale/flip), delete, clip, CSG, create/edit entities and their components, group/layer ops, selection ops.
- **Vision:** `screenshot(camera)` returning an image; named cameras (top/front/side/perspective + custom); optional overlays (grid, axes, object ID labels) so the agent can map pixels to objects.
- **Validation tools:** run the issue checker and return structured results (leaks, invalid brushes, missing targets).
- Batch/transaction tool with all-or-nothing semantics and structured errors (what failed, why, how to fix). Structured errors are what lets the agent self-correct.
- Dry-run option for any mutation.
- Session handling: the headless server owns the document; agent, CLI and human can all edit live; the GUI shows agent edits as they happen (change events pushed to every client).

**Core editor loop (GUI)**
- Four-pane layout (perspective + XY, XZ, YZ), with 1/2/3-pane and maximize options.
- 3D camera: look, pan, orbit, fly keys. 2D views with linked zoom that keeps the point under the cursor fixed.
- Grid with power-of-two sizes and fractional sizes; snapping for nearly all operations.
- Selection: click, add/remove, paint-select, select all/invert, drill through occluded objects.
- Basic transforms: move, rotate, scale, flip.
- Simple shape tool: drag out a cuboid.
- Basic inspector (transform and components) and outliner.

### Tier 1 — A real brush editor

- **Clip tool:** keep-front / keep-back / split; snaps to faces in 3D, to the grid in 2D. Built in the GUI (key `7`): a local preview from `clip_brush`, applied as one `brush.clip`; see `docs/gui.md`.
- **Vertex, edge and face tools:** move components; vertex fusing; face chopping so brushes never go concave.
  - Built as commands: `vertex.translate` and `face.translate` work on brushes by rebuilding the convex hull of the moved corners (`editor_geom::move_brush_vertices`). The GUI has vertex, edge and face tools (keys 6, 8, 9) that drag corners, edge midpoints and face centres in every view, sent as `vertex.translate`; see `docs/gui.md`. Texture lock keeps face UVs on the geometry through these edits and through transforms; see `docs/materials.md`.
- **CSG:** `csg.subtract`, `csg.union` (convex merge for brushes), `csg.intersect` and `csg.hollow` are built; see `docs/tier0-commands.md`.
- **Entities:** an entity is a pose (origin, angles) plus named components; an entity that owns brushes is a brush entity, with its origin as the pivot. Built: components are open JSON; component definitions (our own JSON file, absolute, scene-relative or embedded) carry a JSON Schema per component and drive defaults, validation warnings, marker colour and size; built-in components (`keyframe_animation`, `player_start`); component browser with drag-and-drop; `editor.linkType = entityID` links with arrows in the viewports and screenshots. FGD import and a form generated from the schema are still to do. See `docs/entities.md` and `docs/components.md`.
- **Play mode:** `editor-sim` runs entities as an ECS (static brushes and meshes stay static geometry) and `editor-game` is a separate first-person client for it; the editor never animates anything. Built: keyframe animation on a loop, noclip fly controls, live updates from the server session, Play button (F5), `view.screenshot` with `simulate`. Still to do: triggers and events, several animations per entity, collision, culling. See `docs/ecs.md` and `docs/game.md`.
- **Groups** (nestable, open/close with lock semantics) and **layers** (hide, lock, omit from export).
- **Hide / isolate / lock / show all** (all undoable). Built: `object.set_hidden`, `set_locked`, `isolate`, `show_all`, `unlock_all`, saved in the scene file; locks are enforced for every op in one place. Group open/close is document state (`group.open`, `group.close`; undoable, not saved): everything outside the open group is locked and new objects join it. `layer.set_omit_from_export` has an outliner toggle.
- **Materials:** material browser, per-face material assignment, face attribute editor (offset, scale, rotation). Built: materials are document objects with an image (absolute, relative to the scene file, or embedded) and tint, emissive, roughness, metallic, filter and flags; faces have UVs with optional explicit axes, texture lock, `face.align_uv` / `face.transfer_uv` / `face.get_uv`, a shared face selection, textured rendering and a Surface tool plus inspector panels in the GUI. See `docs/materials.md`. A materials directory and search paths still wait for per-project config.
- **Clipboard:** built. One server-wide clipboard of scene-format text (`clipboard.copy`, `clipboard.get`, `clipboard.paste` with `at` / `offset`); the GUI has Ctrl+C/X/V (at the cursor), Ctrl+Shift+V (in place) and Ctrl+D, and mirrors the text to the system clipboard. See `docs/tier0-commands.md` and `docs/gui.md`.
- **Save/load robustness:** built. The server autosaves dirty sessions and snapshots timed backups into an XDG state directory, flushes on a forced stop, and `recovery.list` / `restore` / `discard` (and a GUI banner) bring work back after a crash. See `docs/server.md`.

### Tier 2 — Productivity

- **Shape tools:** stairs, arch, cylinder, cone, spheroids; complex-shape tool (convex hull from clicked points).
- **Extrude** (drag a face along its normal, with split mode) and **sweep** (arc / straight / S-bend path).
- **UV editor:** drag offset/scale/rotate in the viewport, a UV-space view, auto-alignment on new faces and re-fitting through clip and CSG. (Fit, justify, flip, wrap and texture lock for transforms and vertex/face edits are built.)
- **Linked groups:** duplicates that stay in sync, with per-property protection.
- **Command repetition** (macro-like "do that again", e.g. stairs by duplicate-move-rotate).
- **Issue browser:** live list of problems with quick fixes and click-to-select.
- **Rebindable shortcuts**, configurable mouse mapping, tablet mode.
- **Entity model display** and entity icons/gizmos.
- **Crash recovery:** the autosave covers a killed server; a crash dump of the editor's own last-good document is still to do.
- **Agent extras:** change events pushed to the MCP agent as notifications (clients already get them over the socket), named bookmarks/cameras, "explain this object" semantic descriptions, prefab/library queries.

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
- In-editor play/preview of a minimal runtime. (The first version is the separate `editor-game` client; embedding it in the editor window is still open.)
- Scripting/plugin API beyond MCP (e.g. Rust or WASM extensions).
- Multi-user collaborative editing.
- Additional map formats (Quake 2/3, Hexen 2).

## 5. Scene format

Plain-text TOML, `.scene.toml`, single file in Tier 0, diffable and versioned. Brushes store face planes; meshes store vertices and face loops. Full spec: [scene-format.md](scene-format.md).

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
2. **egui looks and feel. Spiked: acceptable with our own style.** Themed egui with `egui_dock` and four live wgpu viewports works and reads as an editor; no theme crate fits egui 0.36 (catppuccin-egui stops at 0.33), so we own the style in `theme.rs`. Icons are still text. Fallbacks if it stops being enough: more custom drawing inside egui, or a different UI layer later (the command architecture keeps this swappable). Details in [gui.md](gui.md).
3. **egui + winit + wgpu version alignment. Spiked: works.** egui/egui-wgpu/egui-winit 0.36.2, egui_dock 0.21.1, wgpu 30.0.1 and winit 0.30.13 build together with one wgpu. egui 0.37 is out, but `egui_dock` 0.21 holds us on 0.36.
4. **Input routing across panes. Spiked: works.** Per-pane hover and focus, drag capture across pane borders, and key isolation from text fields all hold under `egui_dock`; two traps are recorded in [gui.md](gui.md). Still open: gizmo hit-testing (no gizmos yet), cursor lock for mouse-look, HiDPI and Wayland.
5. **Headless/offscreen rendering** for the CLI and CI on machines without a display. **Spiked: works.** `Renderer::new` requests an adapter with no surface (retrying with `force_fallback_adapter`) and renders to an offscreen texture; the editor-render tests and `editor-cli screenshot` pass with `DISPLAY` and `WAYLAND_DISPLAY` unset on Vulkan (NVIDIA). Still open: a machine with no GPU. This one has no software Vulkan driver (lavapipe), and the GL backend via Mesa EGL fails device creation, so a CI runner needs lavapipe (`mesa-vulkan-drivers`) installed. `WGPU_BACKEND` and `WGPU_ADAPTER_NAME` pick the adapter.
6. **Agent usability.** The tool surface needs real trials with an agent: too fine-grained wastes turns, too coarse is brittle. Plan to iterate on it.
7. **Mesh CSG robustness. Spiked: fixed cases and the donut pass; random slanted input fails about 0.02% of the time (chains only) and is reported as `CsgError::InvalidResult`. Exact predicates are the upgrade path if that proves too high.** Concave polygon meshes (BSP CSG, welding, snapping, n-gon triangulation, non-planar faces) are a second geometry kernel next to the plane-based one. Spike with randomized tests, e.g. donut = cylinder minus cylinder, then vertex edits and re-validation.
8. **Scope.** TrenchBroom is a decade of work. The tiers exist so we ship something useful at Tier 1 and don't need Tier 3 to be happy.

## 8. Suggested order of work

1. ~~Geometry spike (plane brushes, clip, CSG) with tests.~~ Done: `editor-core` types, `editor-geom` brush math and mesh CSG.
2. ~~Document + command layer + native format + undo; CLI over it.~~ Done: `editor-core` document model, `editor-fmt` TOML reader/writer, `editor-cmd` (envelope, transactions, snapshot undo, batch, the full Tier 0 catalog, selection and files in the session, and the schemars-backed registry) and `editor-cli`.
3. ~~Renderer drawing brushes to an offscreen texture; screenshots from the CLI.~~ Done: `editor-geom` camera math, `editor-cmd` `view.camera.*` and `view.frame`, `editor-render` (flat/shaded/wireframe, grid, axes, bounds, id labels, highlight) and `editor-cli screenshot`. `view.screenshot` is client-side because `editor-cmd` stays GPU-free; the server's hub calls `editor_render::screenshot_with_selection` the same way. The `selection` overlay arrived with selection state. Labels are occlusion-aware (an object id buffer), with hide, clip-height, colour modes, a legend and a `path` to save the PNG.
4. ~~MCP server on the command API (now an agent can build and view a scene headlessly).~~ Done: `editor-mcp` (tools generated from `OPS`, image content for `view.screenshot`, read/mutate/destructive flags). Args schemas only; result schemas come later. See [tier0-mcp.md](tier0-mcp.md). It was first a standalone process with its own session; the shared server below replaced that.
4b. ~~Headless server: every client on one session.~~ Done: `editor-server` over a unix socket (`editor-proto` wire and config, `editor-host` `Hub`), with the MCP shim, CLI and GUI as clients, change events pushed to all clients, per-connection transaction ownership (`SessionBusy`) and `merge_key` undo merging. See [server.md](server.md).
5. egui shell with four viewports, camera, selection, grid, move/cuboid tool. Spike done: `editor-app` has the four docked viewports on a shared device, navigation, picking, grid snapping, and Select/Move/Box tools that emit commands (a Move drag is one transaction). The GUI is now a client of the shared server, so agent edits show up live. Still to do: the real tool set and gizmos. See [gui.md](gui.md).
6. Tier 1 tools one at a time, each landing as commands first, then GUI.

Building the headless path (steps 2–4) before the GUI keeps the architecture honest: if an agent can build a scene without the GUI, the GUI is just another client.

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
