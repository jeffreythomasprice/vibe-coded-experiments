# GUI shell (`editor-app`)

The result of the 7.2 (egui look), 7.3 (version alignment) and 7.4 (input routing) spikes, and the shell they left behind. The code is spike quality: it proves the architecture and is meant to be grown, not thrown away, but the tool set is a stub.

Run it with `cargo run -p editor-app [scene.toml]`; it connects to `editor-server` (starting it if needed). With a file it attaches to that file's session. With no argument it joins the server's focused session, so it shows whatever an agent or the CLI already has open, and falls back to `scenes/test.scene.toml` (two rooms, a corridor, stairs, a ramp, a pillar, entities; built with the MCP server) when there is none.

## Architecture

- `gfx.rs` owns the winit surface, the one wgpu device and queue, `egui_wgpu::Renderer` and a `editor_render::Renderer` built on that same device (`Renderer::from_device`).
- Each visible pane has an `Rgba8Unorm` texture on that device, registered with egui as a native texture and shown with `painter.image`. `Renderer::draw_viewport` renders into it (4x MSAA resolved into the pane texture): no readback on the display path. The offscreen readback path (`Renderer::draw`, `view.screenshot`) is unchanged.
- `editor_render::PreparedScene` turns the document into draw lists once per change; every pane draws from it. It is rebuilt only when the revision, selection or hover changes.
- Picking uses the object id buffer (`Renderer::pick_map`, `draw_ids`), cached per pane by (revision, camera, size). It costs about 0.2 ms per refresh.
- The editor is a client of [`editor-server`](server.md) through `remote.rs`. `Remote` keeps a replica of the attached session (document, selection, revision, file and history state): it is built from the server's `snapshot` and kept current by `changed` events, applied at the start of each frame and right after each of the editor's own requests. The document is therefore always local and cheap to read for rendering and the outliner, and an edit by an agent or the CLI shows up without any interaction (the client's reader thread requests a redraw). Tools emit ops (`select.set`, `brush.create_box`, `transform.translate`, `object.delete`, `history.undo/redo`, `session.attach`, `file.save`); nothing mutates the replica directly.
- A Move drag is one undo step without holding a transaction: each snapped step sends an incremental `transform.translate` (live preview) with the drag's `merge_key`, and the server folds the steps into one entry. Esc sends the inverse translate with the same key, which folds to nothing and leaves no redo entry. One Ctrl+Z undoes the whole drag. Because nothing is locked mid-drag, an agent can keep working meanwhile.
- When another client holds a transaction open the status bar says who (`mcp has a transaction open (build)`), and the server refuses the GUI's edits with `SessionBusy` until it ends. File > Open attaches to the chosen file's session (joining it if open elsewhere).
- Layout is `egui_dock` with the four panes as tabs (`PaneId`), plus 1/2/3/4-pane presets and maximize (Space). The outliner (left) and inspector (right) are side panels with a bounded width: a widget asking for unbounded width otherwise stretches the panel over the viewports.
- Requests use the typed argument structs in `editor_cmd::args` (`Editor::call`), and replies are read as the op's `results::*` type. Nothing in the editor builds `json!`.
- Every drag is a `DragOp` running total (`drag.rs`): translate, rotate or scale. The editor sends only the step from the last applied total, with the drag's `merge_key`, about a pivot fixed when the drag begins. Esc sends the inverse of the whole total. The 2D Move, Rotate and Scale tools and the 3D gizmo share this path.
- The gizmo (`gizmo.rs`) is math and drawing only: it hit-tests in screen space and turns pointer rays into a `DragOp`. A drag keeps its own copy of the gizmo, because a move changes the selection bounds the live gizmo is built from.
- Picking by ray (drill-through, the box tool's drawing plane) runs `editor_cmd::ray_hits` on the local replica, so it needs no round trip.
- `panels.rs` draws the outliner and the object inspector from the replica and returns `Event`s; the editor turns them into ops. `tree.rs` builds the outliner tree: an object sits under its entity, else its group, else its layer.
- Hidden objects are left out of the prepared scene (`Document::hidden_ids`), so they are neither drawn nor in the pick map. Each outliner row has `H` and `L` toggles for the object's own flags (layer rows also an `X` toggle for omit from export), and a row is dimmed when hidden (itself or through a container) and italic when locked. A click on a locked object (including one outside the open group) selects nothing: `Pane::hit` filters with `Document::pickable`.
- A click selects the outermost group around the object under the cursor (`Document::pick_target`), and the tint covers the group's members. Double-clicking opens that group (`group.open`): clicks then select its members (or subgroups), everything outside it is locked, and new objects join it, until Esc (`group.close`). The open group is document state, so it is undoable, shared with other clients and shown in the status bar and as `(open)` in the outliner.
- The Vertex, Edge and Face tools (`elements.rs`, keys 6, 8, 9) draw a handle on each corner, edge midpoint or face centre of the selected brushes, in every view. Grabbing one picks every handle that projects within 9 px of the press, so stacked ones move together (an edge seen end-on, the two faces of a box seen from above), and takes all their corners. The drag reuses the Move tool's `DragOp::Translate` steps, sent as `vertex.translate` with `brush_vertices` and the drag's `merge_key`; an edge or face move is just its corners moving, and the server's `move_brush_vertices` fuses corners and splits faces. The picked positions follow the corners after each step (duplicates from fusing are dropped), and Esc sends the inverse. Brush corners have no ids, which is why positions are used. In the perspective view the drag moves on the axis plane that faces the camera most, snapped to the grid, so it cannot move along the view axis (use a 2D view for that); with the Face tool a press anywhere on a selected brush's face (found with `ray_hits`) grabs that face. The hot handle's edge or face outline lights up. `face.translate` stays command-only because these tools send the equivalent corner moves.
- The Clip tool (`clip.rs`, `Pane::clip_tool`) places up to three clip points, kept in `Editor.clip` so they carry across panes. In a 2D view a click puts a point at the snapped (u, v) and the second point makes a plane that contains the view direction; in the perspective view a click picks the face under the cursor with `ray_hits` and snaps the point to the grid within that face's plane, and the second point makes a plane that contains the first face's normal. A third click gives an explicit three-point plane. Dragging a point handle moves it (a 3D drag moves it on the axis plane most facing the camera). The preview is local: `editor_geom::clip_brush` runs on the replica for each selected brush and the resulting edges are drawn in all panes (what is kept in the accent colour, the other side dimmed, or orange for Both). Applying sends one `brush.clip` (`Keep` front, back or both) and with Both selects the new piece too. The plane's front is the side the normal `(c - a) x (b - a)` points into; Shift+Enter swaps sides by reversing the points.
- The Surface tool (key `0`) selects faces instead of objects, through the server's shared face selection (`select.faces.*`), so an agent sees and sets it. A click finds the first pickable brush or mesh face under the cursor with `ray_hits` (`Pane::surface_face`), in any view. Shift toggles, Ctrl+click assigns the material chosen in the inspector, Ctrl+Shift+click wraps the first selected face's image onto the clicked face, arrow keys nudge the offset by the grid (Shift 1 texel) and Page Up/Down rotate 15°. Selecting faces clears the object selection and the other way round. Selected faces draw tinted orange with an outline (`PreparedScene` takes them in `SceneEnv`).
- `surface.rs` draws the face inspector and the material browser/editor and returns its own `Event`s: field edits become `face.set_uv` in `relative` mode (so editing one field of several faces changes only that field), buttons become `face.align_uv` or relative `face.set_uv`, thumbnails are decoded through the renderer's shared `TextureCache` and cached per material by image key. "Pick image…" uses `rfd`. See [materials.md](materials.md).
- The toolbar's **Tex lock** toggle (on by default) is sent as `texture_lock` on every transform and vertex/face drag.
- The replica itself is `editor_proto::Replica` (shared with `editor-game`; see [game.md](game.md)); `Remote` wraps it and derefs to it.
- Component definitions are resolved by the server (it reads the files, relative to the scene), so the replica fetches `component_defs.list` (`Remote::refresh_defs`) on connect and whenever a `d` record or the file path changes, builds an `editor_cmd::component_defs::Catalog` from the resolved components, and bumps `defs_generation` (part of the `PreparedScene` key). The Reload button refetches after a definitions file changes on disk. Link arrows and markers come from that catalog: `PreparedScene` takes the components' looks and the `EntityLink`s in `SceneEnv`, and draws the arrows on top of the scene. A selected entity that owns geometry also gets a small pivot cross at its origin.
- The inspector shows an entity's origin and angles (with "Move pivot only" for an entity that owns geometry, which sends `members: keep`) and one collapsible section per component (`Panels::component`). Each is a JSON text area, kept per (entity, component) in `Panels.json` and re-seeded from the document only while the user has no unsaved edit. `check_json` classifies the text (syntax error, not an object, a `null`, or an object plus its schema violations); only the first three disable Apply, which sends `entity.set_component`. Remove sends `entity.remove_component`, and the `+ add` combo sends `entity.add_component`. Warnings from ops (schema, unknown component) land in the status bar.
- The left panel has Outliner and Entities tabs. The Entities tab is the component browser (`Panels::browser`): `Ui::dnd_drag_source` carries a `ComponentDrag` payload, and `Pane::show` accepts it with `Response::dnd_release_payload` (it uses `contains_pointer`, since `hovered` is false while another widget is dragged) and turns the pointer into a point with `Pane::drop_point`. The editor then sends `entity.create` with the component's schema defaults (`Editor::place_component`). A drag source senses only drags, so placing without dragging is a `+` button instead of a double-click; it adds the component to the selected entities, or makes an entity of the selected world geometry (`entity.create_brush_entity`), or places a point entity. "Entity from selection" makes a component-less entity of the selected geometry.
- Play (menu bar, or F5) starts the `editor-game` binary that sits next to the editor's own executable with `--session <current session id>`; the game shows the live document and the editor never runs the simulation itself.
- `theme.rs` holds the style: our own dark and light palettes, and stock egui dark for comparison (View > Theme).

## Controls

| Where | Input | Does |
|---|---|---|
| Perspective | RMB drag | look; with W A S D Q E fly (Shift faster) |
| Perspective | MMB drag | pan |
| Perspective | Alt + LMB drag | orbit the scene centre |
| Any view | Surface tool, click / Shift+click | select a face / toggle it |
| Any view | Surface tool, Ctrl+click / Ctrl+Shift+click | give the face the inspector's material / wrap the first selected face's image onto it |
| Any view | Surface tool, arrows, Page Up/Down | nudge the offset of the selected faces (Shift: 1 texel), rotate 15° |
| Perspective | wheel | dolly |
| Perspective | Move, Rotate or Scale tool, drag a gizmo handle | move along an axis, rotate about it, scale along it |
| Perspective | Box tool, LMB drag | draw a rectangle on the face under the cursor (the ground if none), grown out by the depth |
| 2D views | MMB or RMB drag | pan |
| 2D views | wheel | zoom, keeping the point under the cursor fixed; the other 2D views follow (View > Link 2D zoom) |
| 2D views | Box tool, LMB drag | create a brush; depth on the missing axis comes from the selection, else the last box, else 64 |
| 2D views | Move tool, LMB drag | move the selection, snapped to the grid |
| 2D views | Rotate tool, LMB drag | rotate the selection about its centre, in 15° steps (Shift: free) |
| 2D views | Scale tool, LMB drag | scale from the opposite corner, so the new size is a whole number of grid steps |
| Any view | Vertex, Edge or Face tool, LMB drag a handle | move the corners, edge or face of the selected brushes (handles that project onto the same spot move together), snapped to the grid; in the perspective view on the axis plane facing the camera, and a press on a face grabs it (Face tool). One undo step per drag. A move that would make the brush concave is refused with the server's message in the status bar |
| 2D views | Clip tool, LMB click | place a clip point at the grid; two points cut along the view direction |
| Perspective | Clip tool, LMB click | place a clip point on the face under the cursor, snapped to the grid in that face; two points cut perpendicular to the first face |
| Any view | Clip tool, LMB drag a point | move the clip point |
| Any view | LMB click | select (Shift adds or removes) |
| Any view | Ctrl + LMB click | select the next object along the ray, wrapping (Shift adds) |
| Any view | LMB double-click | open the group under the cursor to edit its members |
| Any view | drag a component from the Entities tab | create a point entity with that component under the cursor |
| Anywhere | F5 | open the scene in the game client |
| Any view | Shift + LMB drag, Select tool | paint-select |
| Anywhere | Ctrl+A, Ctrl+I, Ctrl+Shift+A | select all, invert, none |
| Anywhere | 1 2 3 4 5 6 7 8 9 | Select, Move, Box, Rotate, Scale, Vertex, Clip, Edge, Face |
| Clip tool | Enter, Shift+Enter, Tab, Backspace, Esc | apply the clip, swap front and back, cycle keep Front / Back / Both, remove the last point (Delete the selection once none are left), clear the points |
| Anywhere | Ctrl+H, Alt+H, Ctrl+Shift+H | hide the selection, show all, isolate the selection |
| Anywhere | Ctrl+L, Alt+L | lock the selection (unlock it if all of it is locked), unlock all |
| Anywhere | `,` `.` | rotate the selection 90° in the active 2D view (about Z in the perspective view) |
| Anywhere | Alt+X, Alt+Y, Alt+Z | flip the selection about its centre |
| Anywhere | `\` | snap on or off |
| Anywhere | `[` `]` | grid size, 0.125 to 256 |
| Anywhere | F | frame the selection, or everything; the 2D views then share the smallest zoom |
| Anywhere | Space | maximize the pane under the cursor |
| Anywhere | Del | delete the selection |
| Anywhere | Ctrl+Z, Ctrl+Shift+Z | undo, redo |
| Anywhere | Ctrl+C, Ctrl+X | copy, cut the selection to the server's clipboard (and the system clipboard, as scene text) |
| Anywhere | Ctrl+V | paste at the cursor: on the surface or ground under it in the perspective view, on the grid in a 2D view; with the cursor outside the views, one grid step from where it was copied |
| Anywhere | Ctrl+Shift+V | paste where it was copied |
| Anywhere | Ctrl+D | duplicate the selection one grid step along +X +Y and select the copies |
| During a drag | Esc | cancel it (moves the objects back) |
| With a group open | Esc | close it |

Shortcuts are ignored while a text field has focus.

**Clipboard:** the Edit menu has Cut, Copy, Paste, Paste in place and Duplicate. Copy sends `clipboard.copy`, then writes the returned text to the system clipboard. Paste uses the system clipboard text only when it is scene text the GUI did not write itself (copied from elsewhere); otherwise it sends no text and the server's clipboard is used, so a later copy by an agent wins. The pasted objects become the selection. Cut is copy plus delete. In the perspective view the paste point is the one a dragged component gets (`Pane::drop_point`), and the centre of the pasted objects goes there.

**Recovery:** when a scene opens and the server holds an autosave for it that is newer than the file and not owned by a running session, a bar under the menu offers Recover (`recovery.restore`; the document becomes unsaved), Discard (`recovery.discard`) and Later. The window title is the scene name, with ` *` while unsaved.

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

- Controls to settle before building more tools: marquee select. Dragging a texture across a face in the viewport is not built (offset fields, nudge keys and buttons only).
- Cache MSAA/depth textures per pane size, and skip redrawing panes whose camera and document are unchanged.
- Cursor lock (winit `set_cursor_grab`) for mouse-look.
- Outliner: reveal every selected object, not just the first, and add rename and drag to regroup.
- Keys that need a modifier fire only when xdotool holds the modifier down explicitly (`keydown ctrl`, `key z`, `keyup ctrl`); a one-shot `key ctrl+z` is missed in the virtual X server.
