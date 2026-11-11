# Tier 0 — Command API

Every operation is a typed, serializable command. The GUI, CLI and MCP server are all clients. See [model](tier0-model.md) for object kinds and IDs.

## Envelope

Same for socket, CLI and MCP:

```json
{ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [64,64,64] }, "dry_run": false, "expect_revision": 41, "merge_key": "drag-3" }
```

`expect_revision` and `merge_key` are optional; `merge_key` folds consecutive edits into one undo step (see [semantics](#semantics)). Vectors are `[x,y,z]`; IDs are prefixed strings.

Success:

```json
{ "ok": true, "revision": 42, "result": {}, "changes": { "created": [], "modified": [], "deleted": [] }, "warnings": [] }
```

Error: `{ "ok": false, "code", "message", "target"?, "field"?, "hint", "index"? }` (`index` is the failing command inside a `batch`), backed by a `CommandError` enum whose `ErrorCode` (a string enum in the error schema) is:
`NotFound`, `WrongKind`, `InvalidArgument`, `InvalidGeometry`, `Locked`, `RevisionMismatch`, `NoTransaction`, `TransactionOpen`, `UnsavedChanges` (`file.new` / `file.open` over unsaved changes without `discard`), `SessionBusy` (another client holds an open transaction, with `holder` and `label` in the message), `FileInUse` (the path is open in another session; join it with `session.attach`), `NotPermitted` (the client was not granted that safety class, see [tier0-mcp.md](tier0-mcp.md)), `Io`, `Format`, `Render` (no graphics adapter, device or readback failure), and the client-side `ServerUnavailable` and `Protocol` (cannot reach the [server](server.md), or its protocol version differs).

Mutating commands report what they created as `result.ids` (in creation order) and `result.id` (the first one, when any). Batch references read these. Objects a command only modified or deleted appear in `changes`, not in `result.ids`.

Ops that replace or edit existing objects (`brush.clip`, `csg.*`, `brush.to_mesh`, `mesh.to_brush`) also return `result.map`: `{input_id: [resulting ids]}`. An id that maps to itself was edited in place, an empty list means it was deleted. Ops that only edit in place (`transform.*`, `face.*`, `entity.*`) return counts, and `changes.modified` is authoritative.

Argument errors (`InvalidArgument`) carry the full path to the offending value in `field` (`faces[0]`) and, when it can be derived from the op's schema, a `hint` naming the expected shape with an example (`expected {verts: [integer], material?: string}, e.g. {"verts":[0]}`). The ops table also accepts underscored names (`brush_create_box`) wherever an op name is expected.

## Registry

`editor_cmd::OPS` is the single source of truth, generated with `dispatch` from one `ops!` table in `crates/editor-cmd/src/commands/mod.rs`. Each entry declares:

- `name`, `summary` (one line; becomes the MCP tool description) and `args_schema` and `result_schema`, JSON schemas derived with `schemars` from the args struct and from the typed result struct the handler returns (`CmdResult<R>`, see `commands/results.rs`; the `ops!` row names both types, so a handler cannot drift from its declared result). `response_schema` wraps the result in the full `Success` envelope. All of them reject unknown fields. Two result fields stay open objects because their shape depends on the object: each entry of `scene.get` / `scene.list` `objects` (always has `id`) and the cameras in `view.camera.*` results. The per-op shapes are documented below. `tests/result_schemas.rs` runs every document and session op and validates the response against its `response_schema`, and fails if a new op has no fixture.
- `safety`: `read`, `mutate` or `destructive`.
- `exec`, where the op runs:
  - `Document`: `dispatch` runs it through a `Ctx`. These are the only ops that can be batched or dry-run, and they are undoable unless `read`.
  - `Session`: `Session` handles it itself (`history.*`, `select.*`, `file.*`). Not batchable, no dry run, no undo entry.
  - `Client`: needs a renderer. Only `view.screenshot`. `editor-cmd` never executes it (`dispatch` answers `InvalidArgument` with a hint); the server's `Hub` calls `editor_render::screenshot_with_selection`.
  - `Server`: `session.list`, `session.attach`, `session.info`. They concern the server's set of sessions, so only the `Hub` runs them; a bare `Session` answers `InvalidArgument`. Not batchable. See [server.md](server.md#sessions).

MCP tools and the CLI's `ops` / `schema` commands are generated from the registry. Safety gating is enforced per connection by the server (`editor-host`), see [server.md](server.md#policy) and [tier0-mcp.md](tier0-mcp.md). Ids in schemas are strings with a prefix pattern; vectors are `[x, y, z]`.

## Catalog

Argument schemas come from the registry (`editor-cli schema <op>`); this section documents results, errors and semantics. Tier tag in parentheses.

### `file.*` (T0)
The session owns the open document and its path.
- `file.info` (read) → `{path, name, dirty, revision}`. `dirty` means the revision differs from the last save or load (undoing back to the saved state still counts as dirty).
- `file.save {path?, overwrite?}` (mutate) writes atomically, creating missing parent directories. With `path` it becomes the document path (save as); without one the document needs a path already. When the document is still named `untitled` and had no path, the name becomes the file stem. Saving to a different, existing file fails with `FileExists` unless `overwrite` is true. Rejected while a transaction is open (`TransactionOpen`).
- `file.new {name?, path?, discard?}` (with `path` the empty document is bound to it, name defaults to the file stem, nothing is written until `file.save`, and an existing file fails with `FileExists`) and `file.open {path, discard?}` (destructive) replace the document. They fail with `UnsavedChanges` while dirty unless `discard` is true, and with `TransactionOpen` inside a transaction. They clear undo, redo and selection and bump the revision so a stale `expect_revision` fails.
- `file.export` is T3.

### `session.*` (T0, server)
Which document a connection is working on; see [server.md](server.md#sessions). All are `read`, and every client must `session.attach` before any other op.
- `session.attach {id?, path?, create?}` → the session row `{id, path, name, dirty, revision, focused, clients, created}`. By `path` it joins or loads that file (`create` writes a missing one, and needs mutate access); with neither it joins the focused session.
- `session.list` → `{sessions: [row], focused}`.
- `session.info` → the row of this connection's session.

### `scene.*` (T0, read)
- `summary` → `{name, counts, bounds, layers, classnames, selection, hidden, locked}`. `selection` is the selection size; `hidden` and `locked` count objects hidden or locked directly or through a container; each layer row also has `hidden`, `locked` and `omit_from_export`.
- `list {kind?, layer?, group?, entity?, classname?, name?, within?, intersects?, hidden?, locked?, fields?, limit?, cursor?}` → `{objects, next_cursor}`. Filters combine with AND. `hidden` and `locked` match effective state, so `{locked: true}` lists everything under a locked layer too. A summary has `hidden` / `locked` keys only for objects that carry the flag themselves. `layer`, `group` and `entity` match the direct parent pointer. `name` is an exact group, layer or camera name. `within` / `intersects` are `{min, max}` boxes tested against object bounds (objects without bounds never match). `fields` keeps only those keys of each summary (`id` always stays). Objects come in id-number order; `limit` defaults to 100 (max 1000); pass the previous `next_cursor` as `cursor`. `next_cursor` is null on the last page.
- `get {ids, detail: summary|full|geometry}`
- `bounds {ids}` → `{bounds: [{id, bounds|null}], combined}`. A group, brush entity or layer covers all its members (nested groups and brush entity members included, cameras excluded); an empty one is null. `scene.get` and the list filters still use an object's own geometry only.
- `raycast {origin, dir, max_dist?, kinds?, ignore?, include_hidden?}` → `{hit: {id, face, point, normal, distance} | null}`. `kinds` is any of `brush`, `mesh`, `entity` (default all). Brushes are exact, meshes are tested triangle by triangle, point entities use their 16-unit box (`face` is null). `distance` is along the normalized `dir`. An origin inside a solid hits the face it leaves through. Hidden objects are skipped unless `include_hidden`.
- `query_box {min, max, mode: intersects|contains, kinds?, include_hidden?}` → `{ids, count}`, by object bounds; hidden objects are skipped unless `include_hidden`.
- `overlaps {ids?, ignore_touching?, ignore_same_group?}` → `{pairs: [{a, b, approx, depth}], count}` for brushes and meshes (all of them when `ids` is omitted). Touching faces, and pieces meeting along an edge or corner, never count: only positive-volume intersections are pairs. Brush pairs are exact (`approx: false`); a pair involving a mesh is tested by bounds only (`approx: true`). `depth` is the thinnest extent of the intersection (of the intersection's bounds for a mesh pair). `ignore_touching` drops pairs with `depth` of 1 unit or less: pieces butted together with a small error, which the always-exact test cannot tell from a real overlap. `ignore_same_group` drops pairs whose two objects sit directly in the same group (pieces of one pillar, one room); objects in different groups, including a group and its subgroup, still count. Both default to false. At most 500 pairs are listed, with a warning when truncated.

### `brush.*`
- `create_box {min, max}` (T0)
- `create_prism {base polygon | sides+radius, axis, height}` (T0; covers cylinders and wedges)
- `create_wedge {min, max, direction, layer?, material?}` (T0). A ramp as one convex brush with five faces: the box `min..max` rising from zero height at the low end to the full box height at the end `direction` (`+x`, `-x`, `+y`, `-y`) points to. The slope is a single plane, so slope angle is set by box length and height.
- `create_from_planes {faces: [{plane, material?}], layer?}` (T0). `plane` is three points as for `clip`. Redundant planes are dropped with a warning; an unbounded or empty result is `InvalidGeometry`.
- `clip {ids, plane, keep: front|back|both, material?}` (T0). The new cut face takes `material`, defaulting to the brush's most common material. `plane` is three points; "front" is the side the normal (c − a) × (b − a) points into. `front` / `back` edit the brush in place (error if that side is empty). `both` keeps the back piece under the original ID and creates the front piece with the same parents. A plane that misses a brush leaves it unchanged and adds a warning.
- `to_mesh {ids}` (T0). Replaces each brush with a mesh: new id, same layer, group, entity and `extra`; the brush id is deleted. Created ids are in `result.ids`.
- Moving faces and vertices with convexity handling: `face.translate` and `vertex.translate`, below.

### `mesh.*`
- `create {vertices, faces: [{verts, material?}], layer?}` (T0). `verts` are zero-based indices into `vertices`, counter-clockwise from outside. Vertices get ids `v1..`, faces `f1..`, vertices snap to the geometry grid. Validation errors fail with `InvalidGeometry`; warnings (open mesh, non-planar face) are returned as warnings.
- `to_brush {ids}` (T0). Like `brush.to_mesh` in reverse; only closed convex meshes convert, otherwise `InvalidGeometry`.
- `delete_faces {faces}` (T0). Faces are `m3/f2` references; vertices no face uses are dropped. Deleting every face is an error (use `object.delete`); opening a closed mesh adds a warning.
- `weld`, `triangulate`, `extrude_faces` (T1/T2)

### Element operations (brushes and meshes)
Faces are element references, see the [model](tier0-model.md#element-references): `b12/f3`, or a bare `b12` for every face.
- `face.set_material {faces, material}` (T0) → `{faces: n}`.
- `face.set_uv {faces, offset?, scale?, rotation?}` (T0). Partial update; give at least one. `offset` and `scale` are `[u, v]`, `scale` must be non-zero.
- `face.translate {faces, delta}`: moves every vertex of the faces, on meshes and brushes.
- `vertex.translate {vertices?, brush_vertices?, delta}`: `vertices` are `m3/v10` references, or a bare mesh or brush id for all of its vertices; give at least one of the two lists. Brush corners have no ids, so `brush_vertices` names them by position: `[{brush: "b12", at: [x, y, z]}]`, matched to the nearest corner within 0.001 (read the positions with `scene.get` at `geometry` detail). `b12/v3` on a brush is `InvalidArgument`.

Both translations snap to the grid, then validate. Meshes: errors fail with `InvalidGeometry` and nothing changes; warnings (such as a face that is no longer planar) are returned as warnings. Brushes stay convex: the result is rebuilt as the convex hull of the moved corners (`editor_geom::move_brush_vertices`). Corners that land on each other fuse, and a face the move makes non-planar splits into several planes. A face keeps its id when its plane or its corner set is unchanged; a split-off face gets a new id and the attributes of the old face it shares most corners with. A move that would leave a corner inside the brush or on an edge fails with `InvalidGeometry` and a hint to use `brush.to_mesh`; a position with no corner is `InvalidArgument`.

### `csg.*` (T1)
`subtract {targets, cutters, output?}`, `union {ids, output?}`, `intersect {ids, output?}`, `hollow {ids, thickness}`.

`subtract` leaves cutters alone unless `consume_cutters` is true, which deletes them in the same step (this raises the required safety to destructive). It returns `result.map` (target -> resulting ids). With brush output, an untouched target stays as is; a target left as one fragment keeps its ID; several fragments replace the target with new brushes (same layer, group and entity); nothing left deletes it. Mesh output (or any mesh operand) turns a brush target into a new mesh and edits a mesh target in place; an empty result deletes the target with a warning. `CsgError::InvalidResult` becomes `InvalidGeometry` with a hint to nudge a cutter.

`union` and `intersect` take two or more distinct brushes or meshes. The first keeps its id and takes the result; the others are deleted. `result.map` maps every input to the result. A mesh output from a brush first operand creates a new mesh in its layer, group and entity. Brush `union` is a convex merge: the convex hull of all the inputs' corners, which can fill space between inputs that do not form a convex shape together (`output: "mesh"` gives the exact union). A hull face on an input face's plane keeps that face's attributes; any other face takes the attributes of the input face with the closest normal. Brush `intersect` keeps the convex volume the brushes share. Inputs that do not overlap fail with `InvalidGeometry` and nothing changes. Mesh operands force mesh output, as for `subtract`.

`hollow` replaces each brush with wall brushes of `thickness`, measured inward from every face: the brush minus the same brush with every face moved inward. A box becomes six walls, in new brushes with the original's layer, group and entity. A mesh id is `WrongKind`; a thickness that is not positive and finite is `InvalidArgument`; a thickness with no room for a cavity is `InvalidGeometry`. `result.map` maps each brush to its walls.

### `room.*`, `stairs.*` (T2 shape tools, built on the T0 primitives)
Coarse ops that expand to ordinary brushes. Each is one `Document` op: one undo step, dry-runnable, batchable, mutate safety. `layer`, `group` and `name` follow `entity.create`: a `group` fixes the layer, and `name` wraps the new brushes in a new group (nested in `group` when both are given), returned as `result.group`. Brushes stay plain world brushes, so `entity.create_brush_entity`, `object.duplicate` and `transform.*` work on them (`"$hall.ids"` in a batch).

- `room.create {min, max, wall, material?, floor_material?, ceiling_material?, wall_material?, layer?, group?, name?, open_sides?}`. `min` and `max` are the **interior** (walkable) volume, so sizes match what an agent wants to stand in; `wall` is the thickness added outside it. Brushes do not overlap: the floor and ceiling span the exterior footprint, north and south walls span the exterior width, and east and west walls fit between them. On an open side the pieces stop at the interior bound on that axis instead of the exterior, so a corridor (`["east","west"]`) is exactly as long as its interior and sits flush against the rooms at its ends without overlapping them (likewise for open north/south). `open_sides` leaves out any of `floor, ceiling, north, south, east, west` (a corridor is `["east", "west"]`; all six is `InvalidArgument`). Result: `ids` in the order floor, ceiling, wall_north, wall_south, wall_east, wall_west (open sides skipped) and `roles`, mapping each of those names to its id. North is +y, east is +x.
- `room.cut_opening {min, max, targets?, material?}`. Cuts a box through brushes: `targets` defaults to every world brush (not a brush entity member) whose bounds the box crosses with positive volume. Explicit targets must be brushes (a mesh is `InvalidArgument` with a hint to use `csg.subtract`). The cutter never enters the document, so nothing is left to delete. New faces inside the opening use `material`, or each target's most common material. Result: `ids` (created fragments) and `map` (target -> resulting ids; a target the box only grazes maps to itself). A box that crosses nothing is `InvalidArgument`. A doorway between two rooms is one call; a corridor is `room.create` with `open_sides: ["east","west"]` plus one `cut_opening` box spanning both end walls.
- `stairs.create {origin, direction, steps, rise, run, width, material?, layer?, group?, name?}`. `direction` is `+x`, `-x`, `+y` or `-y`. `origin` is the floor point at the centre of the first step's low edge. Step i is a solid box from `origin.z` up to `(i+1) * rise`, spanning `run` along the climb and `width` across it (centred on `origin`). Steps do not overlap. Result: `ids` bottom to top and `top`, the point on the floor plane at the far end of the last step, raised by the total rise (where a landing continues).

Considered, not built: an arch. The trial's arch was a few `create_prism` and `clip` calls, and nothing else in the scene repeated it; an arch op would be a curved-opening generator with many parameters (span, rise, segments, thickness) that one scene uses once. Revisit if arches keep recurring. The ramp and `object.array`, the other two candidates, are built (`brush.create_wedge`, `object.array`).

### `transform.*` (T0)
All take `ids` and return `{transformed: n}`. They apply to brushes, meshes and point entities, and recurse through groups and brush entities. Layers and cameras are `WrongKind`. UVs are not adjusted (UV lock is T2). A result outside the world bounds is `InvalidGeometry`.
- `translate {ids, delta}`
- `rotate {ids, axis, degrees, pivot?}`. `axis` is `x`, `y`, `z` or a vector; the rotation follows the right-hand rule.
- `scale {ids, factors, pivot?}`. `factors` is one number or `[x, y, z]`, none zero; negative factors mirror.
- `flip {ids, axis, pivot?}`. Mirrors across the plane through the pivot whose normal is `axis`.

`pivot` is `center` (the default: the centre of the combined bounds), `origin` or `[x, y, z]`. A point entity's origin moves with the transform, and its angles are recomposed when the transform is a pure rotation (scales and mirrors leave angles alone). Mirroring keeps brushes valid by reversing plane winding, and meshes by reversing face loops.

### `object.*` (T0)
- `delete {ids}` (destructive, still undoable).
- `duplicate {ids, offset?}`: deep copy of brushes, meshes, entities and groups, including group members and brush entity members, with parent pointers remapped inside the copy. A copy whose original parent is not copied lands beside the original (same layer, group and entity). `result.ids` lists copies of the requested ids first, then the rest; `result.map` maps every old id to its copy.
- `array {ids, count, offset, mirror?}`: `count` extra deep copies (1..=256) of `ids` at `k * offset` for k = 1..count, as one undoable, dry-runnable op built on the `duplicate` copy path. `offset` must not be zero. `mirror: {axis, pivot}` (`axis` as for `transform.flip`, `pivot` a point) also adds a mirrored copy of the whole row, the originals included (`count + 1` more copies), across the plane through `pivot` with normal `axis`; mirrored brushes and meshes are fixed up exactly as `transform.flip` does. Result: `ids` in creation order (the row copies first, then the mirrored ones; within a copy, the requested ids first and then their contents), `copy_size` (objects per copy, so copy k starts at `ids[k * copy_size]`) and `map`, `{old_id: [copies in that order]}`. At most 10000 objects are created; a copy outside the world bounds is `InvalidGeometry` and nothing is created.
- `rename {id, name}` for groups, layers and cameras (layer and camera names must be unique).
- `set_layer {ids, layer}` moves objects with their contents. An object whose group or entity stays on another layer is an error: move the parent too, or release the object first.

### `entity.*` (T0)
- `create {classname, origin, angles?, properties?, layer?, group?}`. `angles` is `[pitch, yaw, roll]` in degrees. With `group`, the layer is the group's.
- `create_brush_entity {classname, objects, properties?}`: the objects must be world brushes or meshes on one layer and in one group.
- `release_objects {ids}` returns brushes and meshes to the world (or all members, given a brush entity id). A brush entity left empty is deleted, with a warning.
- `set_properties {ids, set?, remove?}`. `classname`, `origin` and `angles` are not properties.
- `set_classname {ids, classname}`
- `set_transform {id, origin?, angles?}` sets a point entity's pose absolutely.

### `layer.*` / `group.*`
- `layer.create {name}`
- `layer.delete {id, move_to?}` (destructive): a layer with contents needs `move_to`; the last layer cannot be deleted.
- `group.create {objects, name?}`: the objects share a layer and parent group and are not brush entity members (group the entity instead).
- `group.ungroup {ids}`: members go to the group's parent.
- `group.move {objects, group?}`: `group` null or absent removes the objects from their group. The target must be on the same layer, and a group cannot move into its own subtree.
- `layer.set_omit_from_export {id, omit}`: stored only; nothing exports yet.

### Hide and lock
All are `object.*`, mutate safety, undoable, and take effect for everything an object contains (see the [model](tier0-model.md#hidden-and-locked)). Brushes, meshes, entities, groups and layers are flaggable; a camera is `WrongKind`.
- `object.set_hidden {ids, hidden}` and `object.set_locked {ids, locked}` → `{updated}`, the number of objects whose flag changed.
- `object.isolate {ids}` → `{updated}`: hides everything except `ids`, what they contain and what contains them, flagging the highest container that leaves those shown (so a whole other layer is one flag, and show-all clears it in one step).
- `object.show_all` and `object.unlock_all` → `{updated}`.
- Hiding drops objects from the selection; `select.set` / `add` on a hidden object is `InvalidArgument`, and `select.all` / `invert` skip hidden and locked objects.

### `select.*` (T0)
Selection is shared session state: not saved, not undoable, no revision bump, never part of a batch. Selectable kinds are brushes, meshes, entities and groups. Every op returns `{ids, count}` (id-number order); the selection is pruned when objects disappear (delete, undo, `file.open`).
- `get` (read), `set {ids}`, `add {ids}`, `remove {ids}`, `clear`, `all {kind?}`, `invert {kind?}` (mutate; `invert` flips within the kind).
- The `selection` screenshot overlay tints the selection.

### `history.*` (T0)
`begin {label}`, `commit`, `rollback`, `undo`, `redo`, `list {limit}` (label, author client, revision, change counts). `rollback` returns what it reverted in `changes`. An open transaction belongs to the connection that began it; others get `SessionBusy` for mutations until it ends or its owner disconnects (which rolls it back).

### `batch` (T0)
Ordered list of `Document` commands run as one transaction, all-or-nothing (session and client ops are rejected inside). Items are `{op, args?, as?}`; `op` may be dotted or underscored. Later items reference earlier results in any string inside `args`:

- `"$N"` or `"$name"`: the primary id (`result.id`) of item N (counting from 0) or of the item with `"as": "name"`. Names are `[A-Za-z_][A-Za-z0-9_]*` and unique per batch.
- `"$N.ids[k]"` / `"$name.ids[k]"`: the k-th created id.
- A `/f<n>` or `/v<n>` suffix selects a face or vertex of that object: `"$m/v2"`.
- `"$$"` at the start of a string is a literal `$`. A `$...` string that matches the grammar but names nothing is an error (the hint lists defined names).

Errors report the failing index.

### `view.*` (T0)
Camera and view conventions are in the [model doc](tier0-model.md#camera). A camera reference is an id (`c5`), a camera name, or a session camera `viewport.persp|top|front|side`.

- `view.screenshot {camera?, width?, height?, mode?, overlays?, highlight?, hide?, show_hidden?, clip_above?, label_ids?, label_kinds?, min_label_pixels?, max_labels?, colors?, material_colors?, path?}` (read, client-side; `path` makes it need mutate access). `camera` is a reference or an inline object with the `view.camera.create` fields (default `viewport.persp`). `width` and `height` default to 1024 x 768, max 4096. `mode` is `shaded` (default; lit, dark feature edges), `flat` (unlit colour, no edges) or `wireframe`. `overlays` is any of `grid`, `axes`, `labels`, `bounds`, `selection` (tints the session selection; `highlight` wins when both name an object). `highlight` is object ids: tinted, with bright edges drawn through occluders.
  - Objects hidden in the document are left out too, unless `show_hidden` is true. `hide` is object ids, groups, brush entities or layers to leave out, and `clip_above` is a world z above which nothing is drawn (a section or plan view). Neither changes the document. There is no `hide_ceilings`: a ceiling is just a brush, and nothing in brush geometry reliably says which ones are ceilings (a slab above a room, a lintel, a bridge). A section view is already one argument, `clip_above` at the wall-top z (a z just under the ceiling's lower face); `room.create` also returns the ceiling id as `roles.ceiling` for `hide`.
  - Sky versus ceiling: the clear colour is the single flat `legend.background` hex (`#292b30`). No surface is ever drawn in exactly that colour, so a pixel of that colour is empty space. Considered, not built: a gradient or distinctly tinted clear colour as a sky cue, because it needs a background pass in the renderer and makes every image differ for little gain over the legend entry.
  - `colors` is `material` (default, a stable hue per material), `orientation` (legend names `faces up (floors)`, `faces down (ceilings)`, `vertical (walls)`; a flat ceiling seen from above is the "faces up" colour of its top, so use this mode to tell ceilings from open sky) or `layer`. `material_colors` maps material names to `"#rrggbb"` and applies in every mode. Point entities have markers by classname: `light*` is a yellow octahedron, `info_player*` a green tall box, anything else a box coloured by classname.
  - `path` also writes the PNG there (parent directories are created, the write is atomic). The image is still returned inline over MCP.
  - Result: `{width, height, camera, legend, labels?, labels_drawn?, labels_omitted?, path?}`. `legend` is `{mode, background, faces: [{name, color}], entities: [{classname, color, marker}]}` for what was drawn. `labels` (only with the `labels` overlay) lists `{id, x, y, pixels}` for exactly the labels drawn, and **`labels` with `labels_drawn` (its length) and `labels_omitted` are the authoritative counts**: do not count text boxes in the image, which can overlap or leave the frame. An object gets a label only when at least `min_label_pixels` (default 12) of its pixels are visible, it passes `label_ids` (groups, entities and layers expand to their contents) and `label_kinds` (any of `brush`, `mesh`, `entity`; default all), and there is room. `x, y` is a visible pixel of the object near the centre of its visible area, and `pixels` is its visible area. Labels are placed largest object first and capped at `max_labels` (default 40). A label whose box would overlap another is moved aside by about its own size, with a yellow leader line from the object's `x, y` to the box; if no nearby spot is free it is dropped. `labels_omitted` counts objects that passed every filter above but got no label (the cap, or no free spot); objects filtered out or too small are not counted. Unrenderable objects add a warning. The CLI writes the PNG and adds `path` (session mode takes `path` too, or returns `png_base64`); `--args` carries `label_kinds` and `min_label_pixels`.
- `view.camera.list` (read) → `{cameras, viewports}`.
- `view.camera.get {camera}` (read) → `{camera}`. Session cameras come back framed on the scene for 1024 x 768, with `id: null`.
- `view.camera.create {name, position, yaw?, pitch?, target?, fov?}` or `{name, ortho, position?, zoom?}` (mutate). `target` sets yaw and pitch (not with yaw/pitch). An ortho camera without position or zoom is framed on the scene for 1024 x 768. Names must be unique, non-empty, not start with `viewport.` and not look like an id. `fov` is in (1, 179), `pitch` within ±89, `zoom` > 0.
- `view.camera.set {camera, name?, position?, yaw?, pitch?, target?, fov?, ortho?, zoom?}` (mutate). `ortho` switches to orthographic, `fov` back to perspective. Session cameras cannot be edited.
- `view.camera.delete {cameras}` (destructive). Cameras only.
- `view.frame {ids, camera, width?, height?}` (mutate). Moves a document camera (and sets the zoom of an ortho one) so the objects fill the image at the given size (default 1024 x 768). Orientation and fov are kept. `ids` may name groups, brush entities and layers (their members' union is framed; one with nothing to frame is `InvalidArgument`). Perspective framing fits the bounds' eight corners into the frustum, so scenes much wider than tall fill the image.

Camera edits are ordinary undoable commands.

### `check.validate {ids?}` (T0)
Returns issues `{code, severity, ids, elements?, message, hint}`. `ids` are objects; `elements` are the faces or vertices involved in the tools' own notation (`m84/v1`), and messages use the same notation. Tier 0 codes: `invalid_brush`, `invalid_mesh`, `degenerate_face`, `mesh_open`, `mesh_nonmanifold`, `mesh_nonplanar_face`, `empty_brush_entity`, `missing_target`, `duplicate_object`. Leak checks wait for a compiler (T3).

## Semantics

- **Transactions:** a batch or `begin`/`commit` pair applies atomically and undoes as one step.
- **Merging:** with no transaction open, a request carrying `merge_key` folds into the top undo entry when that entry was made by the same connection with the same key: the entry keeps each object's earliest "before" and latest "after", and is dropped if they cancel. The revision still advances on every step, `changes` in each response describes that step alone, and anything else (another key, another connection, an undo) ends the chain. GUI drags send one `transform.translate` per snapped step under a fresh key, so a drag is one undo step without holding a transaction; cancelling sends the inverse translate with the same key, which folds to nothing and leaves no redo entry.
- **Revision:** every edit that changes the document advances the revision, including edits inside an open transaction, so `expect_revision` chains mid-transaction. `commit` does not advance it again and its undo entry records the revision at commit. `rollback` advances it once if the transaction changed anything, so a revision is never reused for different content; the rolled-back document counts as clean if it was clean at `begin`.
- **Concurrency:** the [server](server.md) is the single writer, one op at a time across all clients. The revision counter advances on every edit; `expect_revision` rejects stale writes with `RevisionMismatch`. Transactions are exclusive to their connection (`SessionBusy`).
- **Events:** every change to a session, whatever caused it (including edits inside an open transaction, rollback, undo and redo), is broadcast to all clients attached to it as a `changed` event, or a `snapshot` when the document is replaced. See [server.md](server.md#protocol).
- **Attribution:** each undo entry records its originating client name and connection, shown in `history.list`.
- **Dry run:** returns the would-be changeset without committing.
- **Undo storage:** each history entry stores before/after snapshots of every object it touched, so undo and redo work the same for every command. Undo never rewinds the ID counter.
- **File mode:** `editor-cli` loads the file, runs the command in a fresh session and saves, so revision and undo history do not carry from one invocation to the next. A long-lived editor session keeps them.
- <a id="locks"></a>**Locks:** one check covers every op, in `editor_cmd::dispatch` (`commands/locks.rs`), not in the handlers: after an op, anything it edited or deleted that was effectively locked before, or any object it created or moved into a locked layer, group or entity, fails the op with `Locked` (target = the object) and the op is rolled back. Changing only `hidden` / `locked` is always allowed, so an object can be unlocked. A `batch` checks each item, so it can unlock, edit and relock; a failing item still rolls back the whole batch. Duplicating a locked object is allowed (the copy is locked). Undo and redo ignore locks.

## CLI mapping

```
editor-cli ops [--safety read|mutate|destructive] [--json]
editor-cli schema <op> [--result]
editor-cli schema --wire
editor-cli --file <f> new [--name N] [--force]
editor-cli [--file <f>] run <op> --args '{...}' [--dry-run] [--expect-revision N] [--save]
editor-cli [--file <f>] apply batch.json [--dry-run] [--save]
editor-cli [--file <f>] screenshot --camera top -o out.png
editor-cli [--file <f>] session [--autosave] [--read-only] [--events [--full]]
editor-cli [--file <f>] watch [--full]
editor-cli sessions
editor-cli server status | stop [--force]
```

`--config PATH` and `--pretty` apply to every command. The response JSON goes to stdout. Exit code 0 on success, 1 for a command error, 2 for an I/O, file-format, usage, render (`Render`), `ServerUnavailable` or `Protocol` error.

`ops` and `schema` list the registry (`--json` adds scope and undoable) and need no server. `schema <op>` prints the argument schema, `--result` the whole response schema, `--wire` the socket protocol's schemas (see [server.md](server.md#protocol)). `new` writes an empty scene file locally and does not use the server.

Every other command is a client of the [server](server.md), which it autospawns. `--file` attaches to that file's session (loading it if nobody has it open); without `--file` it attaches to the focused session, so `editor-cli run ...` edits whatever the GUI has open, live.

**Saving.** If the command made the session (nobody had the file open), it is a one-shot: after a successful, non-dry-run command the CLI saves the file if it is dirty and disconnects, and the clean session is dropped. If the session was already open elsewhere, the CLI does not save (the other client owns that decision); `--save` forces it. `run` and `apply` without `--file` against a server with no open session fail with `InvalidArgument`.

`run` accepts any op, including `history.*` and `select.*`: they act on the shared session. `view.screenshot` through `run` works too.

`session` holds one connection and reads one request envelope per stdin line, writing exactly one response line each; bad JSON and command errors are response lines, not exits. Undo, transactions, selection, revision and dirty state work as in MCP. `--autosave` saves after every change; `--read-only` connects with a read-only policy. `--events` also prints the session's change events as JSON lines (`{"event":"changed",...}`, `snapshot` without its document unless `--full`), interleaved with the responses. On exit with unsaved changes it prints a warning (the server keeps the session). `view.screenshot` takes an extra `path`; without it the result carries `png_base64`. `watch` attaches and prints events until the server closes.

`screenshot` flags: `--camera` (default `viewport.persp`), `-o/--out` (required; written by the server, atomically), `--width`, `--height`, `--mode`, `--overlay grid,axes,labels,bounds,selection`, `--highlight b1,b2`, `--hide ids`, `--clip-above Z`, `--label-ids ids`, `--max-labels N`, `--colors material|orientation|layer`, `--material-color NAME=#rrggbb` (repeatable), and `--args '{...}'` for the full `view.screenshot` arguments (flags override it). It never modifies the scene. It renders in the server, so the `selection` overlay shows the shared selection, and the `Render` exit code is used when the server has no graphics adapter, so a headless machine needs a Vulkan driver (see the plan, risk 5). Relative paths in arguments resolve against the CLI's working directory.
