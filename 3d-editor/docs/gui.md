# GUI shell (`editor-app`)

The result of the 7.2 (egui look), 7.3 (version alignment) and 7.4 (input routing) spikes, and the shell they left behind. The code is spike quality: it proves the architecture and is meant to be grown, not thrown away, but the tool set is a stub.

Run it with `cargo run -p editor-app [level.toml]`; it connects to `level-server` (starting it if needed). With a file it attaches to that file's session. With no argument it joins the server's focused session, so it shows whatever an agent or the CLI already has open, and falls back to `scenes/test-scene.level.toml` (two rooms, a corridor, stairs, a ramp, a pillar, entities; built with the MCP server) when there is none.

## Architecture

- `gfx.rs` owns the winit surface, the one wgpu device and queue, `egui_wgpu::Renderer` and a `level_render::Renderer` built on that same device (`Renderer::from_device`).
- Each visible pane has an `Rgba8Unorm` texture on that device, registered with egui as a native texture and shown with `painter.image`. `Renderer::draw_viewport` renders into it (4x MSAA resolved into the pane texture): no readback on the display path. The offscreen readback path (`Renderer::draw`, `view.screenshot`) is unchanged.
- `level_render::PreparedScene` turns the document into draw lists once per change; every pane draws from it. It is rebuilt only when the revision, selection or hover changes.
- Picking uses the object id buffer (`Renderer::pick_map`, `draw_ids`), cached per pane by (revision, camera, size). It costs about 0.2 ms per refresh.
- The editor is a client of [`level-server`](server.md) through `remote.rs`. `Remote` keeps a replica of the attached session (document, selection, revision, file and history state): it is built from the server's `snapshot` and kept current by `changed` events, applied at the start of each frame and right after each of the editor's own requests. The document is therefore always local and cheap to read for rendering and the outliner, and an edit by an agent or the CLI shows up without any interaction (the client's reader thread requests a redraw). Tools emit ops (`select.set`, `brush.create_box`, `transform.translate`, `object.delete`, `history.undo/redo`, `session.attach`, `file.save`); nothing mutates the replica directly.
- A Move drag is one undo step without holding a transaction: each snapped step sends an incremental `transform.translate` (live preview) with the drag's `merge_key`, and the server folds the steps into one entry. Esc sends the inverse translate with the same key, which folds to nothing and leaves no redo entry. One Ctrl+Z undoes the whole drag. Because nothing is locked mid-drag, an agent can keep working meanwhile.
- When another client holds a transaction open the status bar says who (`mcp has a transaction open (build)`), and the server refuses the GUI's edits with `SessionBusy` until it ends. File > Open attaches to the chosen file's session (joining it if open elsewhere).
- Layout is `egui_dock` with the four panes as tabs (`PaneId`), plus 1/2/3/4-pane presets and maximize (Space). The outliner (left) and inspector (right) are side panels with a bounded width: a widget asking for unbounded width otherwise stretches the panel over the viewports.
- Requests use the typed argument structs in `level_cmd::args` (`Editor::call`), and replies are read as the op's `results::*` type. Nothing in the editor builds `json!`.
- Every drag is a `DragOp` running total (`drag.rs`): translate, rotate or scale. The editor sends only the step from the last applied total, with the drag's `merge_key`, about a pivot fixed when the drag begins. Esc sends the inverse of the whole total. The 2D Move, Rotate and Scale tools and the 3D gizmo share this path.
- The gizmo (`gizmo.rs`) is math and drawing only: it hit-tests in screen space and turns pointer rays into a `DragOp`. A drag keeps its own copy of the gizmo, because a move changes the selection bounds the live gizmo is built from.
- Picking by ray (drill-through, the box tool's drawing plane) runs `level_cmd::ray_hits` on the local replica, so it needs no round trip.
- `panels.rs` draws the outliner, inspector and material list from the replica and returns `Event`s; the editor turns them into ops. `tree.rs` builds the outliner tree: an object sits under its entity, else its group, else its layer.
- `theme.rs` holds the style: our own dark and light palettes, and stock egui dark for comparison (View > Theme).

## Controls

| Where | Input | Does |
|---|---|---|
| Perspective | RMB drag | look; with W A S D Q E fly (Shift faster) |
| Perspective | MMB drag | pan |
| Perspective | Alt + LMB drag | orbit the scene centre |
| Perspective | wheel | dolly |
| Perspective | Move, Rotate or Scale tool, drag a gizmo handle | move along an axis, rotate about it, scale along it |
| Perspective | Box tool, LMB drag | draw a rectangle on the face under the cursor (the ground if none), grown out by the depth |
| 2D views | MMB or RMB drag | pan |
| 2D views | wheel | zoom, keeping the point under the cursor fixed; the other 2D views follow (View > Link 2D zoom) |
| 2D views | Box tool, LMB drag | create a brush; depth on the missing axis comes from the selection, else the last box, else 64 |
| 2D views | Move tool, LMB drag | move the selection, snapped to the grid |
| 2D views | Rotate tool, LMB drag | rotate the selection about its centre, in 15° steps (Shift: free) |
| 2D views | Scale tool, LMB drag | scale from the opposite corner, so the new size is a whole number of grid steps |
| Any view | LMB click | select (Shift adds or removes) |
| Any view | Ctrl + LMB click | select the next object along the ray, wrapping (Shift adds) |
| Any view | Shift + LMB drag, Select tool | paint-select |
| Anywhere | Ctrl+A, Ctrl+I, Ctrl+Shift+A | select all, invert, none |
| Anywhere | 1 2 3 4 5 | Select, Move, Box, Rotate, Scale |
| Anywhere | `,` `.` | rotate the selection 90° in the active 2D view (about Z in the perspective view) |
| Anywhere | Alt+X, Alt+Y, Alt+Z | flip the selection about its centre |
| Anywhere | `\` | snap on or off |
| Anywhere | `[` `]` | grid size, 0.125 to 256 |
| Anywhere | F | frame the selection, or everything; the 2D views then share the smallest zoom |
| Anywhere | Space | maximize the pane under the cursor |
| Anywhere | Del | delete the selection |
| Anywhere | Ctrl+Z, Ctrl+Shift+Z | undo, redo |
| During a drag | Esc | cancel it (moves the objects back) |

Shortcuts are ignored while a text field has focus.

## Spike outcomes

### 7.3 Version alignment: works

`egui`, `egui-wgpu`, `egui-winit` 0.36.2, `egui_dock` 0.21.1, `wgpu` 30.0.1 and `winit` 0.30.13 build together with a single copy of wgpu. The only duplicates in `cargo tree -d` are `calloop` and its Wayland glue (0.13 from winit, 0.14 from egui-winit's clipboard), which is harmless.

- egui 0.37 is already released, but `egui_dock` 0.21 requires 0.36, so `egui_dock` gates the bump. `egui_stack` (a fork that continues `egui_dock`) exists; I did not evaluate it.
- API drift to know about, since older examples are wrong: egui 0.36 has `Context::run_ui` and `Panel` (not `TopBottomPanel`/`SidePanel`), `global_style_mut`, and `CornerRadius`. wgpu 30 has `get_current_texture() -> CurrentSurfaceTexture` and `queue.present(frame)`. A `TexturesDelta` must be consumed or cleared before it drops, or egui panics.
- We use a non-sRGB surface format because egui blends in gamma space, and the viewport textures hold already-encoded colours, so panes match screenshots.

### 7.2 egui look and feel: acceptable, own the style

About 120 lines in `theme.rs` (palette, spacing, radii, font sizes) take egui from a debug panel to something that reads as an editor. Stock dark is noticeably more cramped and flatter; compare them in View > Theme. `egui_dock`'s tab bars, splitters and drag-to-rearrange work and look fine under the custom style.

- `catppuccin-egui` 5.7 supports egui up to 0.33 only, so it cannot be used with 0.36.
- `egui_styled` is a real crate (0.9.0, Tailwind-style utility styling). Not evaluated: our own `Style` was enough.
- Icons are still text. An icon font (`egui-phosphor` targets older egui; `egui-pop-phosphor` is a fork for 0.37) was not tried.
- The viewport clear colour is theme-independent (dark), which looks fine in the light theme.

Frame cost in a debug build on a software-presented X server: UI 3.9 ms, four panes 4.7 ms, pick 0.2 ms. The `present` figure (20 ms) is the virtual X server's copy, not representative. MSAA and depth textures are reallocated every draw; cache them per size when it matters.

### 7.4 Input routing: works, with these traps

Verified by scripted input in a virtual X server (xdotool):

- Hover and focus are per pane (`Response::hovered`, `has_focus`); only the hovered pane picks, zooms and handles the wheel.
- A drag that starts in a pane keeps going when the cursor leaves it, including over the side panels (egui captures the drag on the widget).
- Typing in the inspector text field does not leak W A S D, Space or Delete to the viewports.
- Tab drag to rearrange the dock works; layout presets and maximize work; undo/redo, tool switching and the grid keys work from the keyboard.
- Zoom-at-cursor holds the point under the cursor.

Traps:

- `Context::egui_wants_keyboard_input()` is true whenever any widget has focus, including a focusable viewport, so using it to gate shortcuts kills them after a click on a pane. Use `text_edit_focused()`.
- `egui_winit::State::on_window_event` returns `repaint: true` for `RedrawRequested`. Requesting a redraw on that spins the CPU at 100% when idle.
- Shortcut handling runs before the panes each frame, so a pane's own key use (fly keys) must not collide with the global keys. Today fly keys only apply while RMB is held.

Not verified (all testing ran in a virtual X server, scale factor 1): a real display and its frame pacing, HiDPI scaling, Wayland, a real wheel or trackpad, dragging a tab out to a floating window, cursor lock while looking, and the pointer on a real display with a gizmo. Gizmo hit-testing and dragging were checked in the virtual X server with xdotool (move, rotate and scale in the perspective view).

## Next

- Controls to settle before building more tools: marquee select, arrow-key nudge, how the clip tool previews, vertex mode, and where material and entity browsers dock. Materials are names only for now (see `docs/plan.md`).
- Cache MSAA/depth textures per pane size, and skip redrawing panes whose camera and document are unchanged.
- Cursor lock (winit `set_cursor_grab`) for mouse-look.
- Outliner: reveal every selected object, not just the first, and add rename and drag to regroup.
- Keys that need a modifier fire only when xdotool holds the modifier down explicitly (`keydown ctrl`, `key z`, `keyup ctrl`); a one-shot `key ctrl+z` is missed in the virtual X server.
