# Tier 0 — Command API

Every operation is a typed, serializable command. The GUI, CLI and MCP server are all clients. See [model](tier0-model.md) for object kinds and IDs.

## Envelope

Same for socket, CLI and MCP:

```json
{ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [64,64,64] }, "dry_run": false, "expect_revision": 41 }
```

`expect_revision` is optional. Vectors are `[x,y,z]`; IDs are prefixed strings.

Success:

```json
{ "ok": true, "revision": 42, "result": {}, "changes": { "created": [], "modified": [], "deleted": [] }, "warnings": [] }
```

Error: `{ "ok": false, "code", "message", "target"?, "field"?, "hint", "index"? }` (`index` is the failing command inside a `batch`), backed by a `CommandError` enum:
`NotFound`, `WrongKind`, `InvalidArgument`, `InvalidGeometry`, `Locked`, `RevisionMismatch`, `NoTransaction`, `TransactionOpen`, `UnsavedChanges` (`file.new` / `file.open` over unsaved changes without `discard`), `NotPermitted` (the client was not granted that safety class, see [tier0-mcp.md](tier0-mcp.md)), `Io`, `Format`, `Render` (no graphics adapter, device or readback failure).

Mutating commands report what they created as `result.ids` (in creation order) and `result.id` (the first one, when any). Batch `$N` references read these. Objects a command only modified or deleted appear in `changes`, not in `result.ids`.

## Registry

`level_cmd::OPS` is the single source of truth, generated with `dispatch` from one `ops!` table in `crates/level-cmd/src/commands/mod.rs`. Each entry declares:

- `name`, `summary` (one line; becomes the MCP tool description) and `args_schema`, a JSON schema derived with `schemars` from the args struct. Results are not schematized: they are documented here.
- `safety`: `read`, `mutate` or `destructive`.
- `exec`, where the op runs:
  - `Document`: `dispatch` runs it through a `Ctx`. These are the only ops that can be batched or dry-run, and they are undoable unless `read`.
  - `Session`: `Session` handles it itself (`history.*`, `select.*`, `file.*`). Not batchable, no dry run, no undo entry.
  - `Client`: needs a renderer. Only `view.screenshot`. `level-cmd` never executes it (`dispatch` answers `InvalidArgument` with a hint); `level-host` calls `level_render::screenshot`.

MCP tools and the CLI's `ops` / `schema` commands are generated from the registry. Safety gating is enforced by `level-host`, see [tier0-mcp.md](tier0-mcp.md). Ids in schemas are strings with a prefix pattern; vectors are `[x, y, z]`.

## Catalog

Argument schemas come from the registry (`level-cli schema <op>`); this section documents results, errors and semantics. Tier tag in parentheses.

### `file.*` (T0)
The session owns the open document and its path.
- `file.info` (read) → `{path, name, dirty, revision}`. `dirty` means the revision differs from the last save or load (undoing back to the saved state still counts as dirty).
- `file.save {path?, overwrite?}` (mutate) writes atomically, creating missing parent directories. With `path` it becomes the document path (save as); without one the document needs a path already. Saving to a different, existing file fails with `FileExists` unless `overwrite` is true. Rejected while a transaction is open (`TransactionOpen`).
- `file.new {name?, path?, discard?}` (with `path` the empty document is bound to it, name defaults to the file stem, nothing is written until `file.save`, and an existing file fails with `FileExists`) and `file.open {path, discard?}` (destructive) replace the document. They fail with `UnsavedChanges` while dirty unless `discard` is true, and with `TransactionOpen` inside a transaction. They clear undo, redo and selection and bump the revision so a stale `expect_revision` fails.
- `file.export` is T3.

### `scene.*` (T0, read)
- `summary` → `{name, counts, bounds, layers, classnames, selection}` (`selection` is the selection size).
- `list {kind?, layer?, group?, entity?, classname?, name?, within?, intersects?, fields?, limit?, cursor?}` → `{objects, next_cursor}`. Filters combine with AND. `layer`, `group` and `entity` match the direct parent pointer. `name` is an exact group, layer or camera name. `within` / `intersects` are `{min, max}` boxes tested against object bounds (objects without bounds never match). `fields` keeps only those keys of each summary (`id` always stays). Objects come in id-number order; `limit` defaults to 100 (max 1000); pass the previous `next_cursor` as `cursor`. `next_cursor` is null on the last page.
- `get {ids, detail: summary|full|geometry}`
- `bounds {ids}` → `{bounds: [{id, bounds|null}], combined}`.
- `raycast {origin, dir, max_dist?, kinds?, ignore?}` → `{hit: {id, face, point, normal, distance} | null}`. `kinds` is any of `brush`, `mesh`, `entity` (default all). Brushes are exact, meshes are tested triangle by triangle, point entities use their 16-unit box (`face` is null). `distance` is along the normalized `dir`. An origin inside a solid hits the face it leaves through.
- `query_box {min, max, mode: intersects|contains, kinds?}` → `{ids, count}`, by object bounds.
- `overlaps {ids?}` → `{pairs: [{a, b, approx}], count}` for brushes and meshes (all of them when `ids` is omitted). Touching faces do not count. Brush pairs are exact (`approx: false`); a pair involving a mesh is tested by bounds only (`approx: true`). At most 500 pairs are listed, with a warning when truncated.

### `brush.*`
- `create_box {min, max}` (T0)
- `create_prism {base polygon | sides+radius, axis, height}` (T0; covers cylinders and wedges)
- `create_from_planes {faces: [{plane, material?}], layer?}` (T0). `plane` is three points as for `clip`. Redundant planes are dropped with a warning; an unbounded or empty result is `InvalidGeometry`.
- `clip {ids, plane, keep: front|back|both}` (T0). `plane` is three points; "front" is the side the normal (c − a) × (b − a) points into. `front` / `back` edit the brush in place (error if that side is empty). `both` keeps the back piece under the original ID and creates the front piece with the same parents. A plane that misses a brush leaves it unchanged and adds a warning.
- `to_mesh {ids}` (T0). Replaces each brush with a mesh: new id, same layer, group, entity and `extra`; the brush id is deleted. Created ids are in `result.ids`.
- `move_faces`, `move_vertices` with convexity handling (T1)

### `mesh.*`
- `create {vertices, faces: [{verts, material?}], layer?}` (T0). `verts` are zero-based indices into `vertices`, counter-clockwise from outside. Vertices get ids `v1..`, faces `f1..`, vertices snap to the geometry grid. Validation errors fail with `InvalidGeometry`; warnings (open mesh, non-planar face) are returned as warnings.
- `to_brush {ids}` (T0). Like `brush.to_mesh` in reverse; only closed convex meshes convert, otherwise `InvalidGeometry`.
- `delete_faces {faces}` (T0). Faces are `m3/f2` references; vertices no face uses are dropped. Deleting every face is an error (use `object.delete`); opening a closed mesh adds a warning.
- `weld`, `triangulate`, `extrude_faces` (T1/T2)

### Element operations (brushes and meshes)
Faces are element references, see the [model](tier0-model.md#element-references): `b12/f3`, or a bare `b12` for every face.
- `face.set_material {faces, material}` (T0) → `{faces: n}`.
- `face.set_uv {faces, offset?, scale?, rotation?}` (T0). Partial update; give at least one. `offset` and `scale` are `[u, v]`, `scale` must be non-zero.
- `face.translate {faces, delta}` (mesh T0, brush T1): moves every vertex of the faces. Brushes give `InvalidArgument` with a hint to use `brush.to_mesh`.
- `vertex.translate {vertices, delta}` (mesh T0, brush T1): `m3/v10` references, or a bare mesh id for all vertices.

Both translations snap to the grid, then validate the mesh: errors fail with `InvalidGeometry` and nothing changes; warnings (such as a face that is no longer planar) are returned as warnings.

### `csg.*` (T1, specified now)
`subtract {targets, cutters, output?}`, `union`, `intersect`, `hollow {ids, thickness}`.

`subtract` leaves cutters alone. With brush output, an untouched target stays as is; a target left as one fragment keeps its ID; several fragments replace the target with new brushes (same layer, group and entity); nothing left deletes it. Mesh output (or any mesh operand) turns a brush target into a new mesh and edits a mesh target in place; an empty result deletes the target with a warning. `CsgError::InvalidResult` becomes `InvalidGeometry` with a hint to nudge a cutter.

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
- Hide, lock, isolate (T1).

### `select.*` (T0)
Selection is shared session state: not saved, not undoable, no revision bump, never part of a batch. Selectable kinds are brushes, meshes, entities and groups. Every op returns `{ids, count}` (id-number order); the selection is pruned when objects disappear (delete, undo, `file.open`).
- `get` (read), `set {ids}`, `add {ids}`, `remove {ids}`, `clear`, `all {kind?}`, `invert {kind?}` (mutate; `invert` flips within the kind).
- The `selection` screenshot overlay tints the selection.

### `history.*` (T0)
`begin {label}`, `commit`, `rollback`, `undo`, `redo`, `list {limit}` (label, author client, revision, change counts).

### `batch` (T0)
Ordered list of `Document` commands run as one transaction, all-or-nothing (session and client ops are rejected inside). Later items reference earlier results with `"$N"` (primary id) or `"$N.ids[k]"`. Errors report the failing index.

### `view.*` (T0)
Camera and view conventions are in the [model doc](tier0-model.md#camera). A camera reference is an id (`c5`), a camera name, or a session camera `viewport.persp|top|front|side`.

- `view.screenshot {camera?, width?, height?, mode?, overlays?, highlight?}` (read, client-side). `camera` is a reference or an inline object with the `view.camera.create` fields (default `viewport.persp`). `width` and `height` default to 1024 x 768, max 4096. `mode` is `shaded` (default; lit, dark feature edges), `flat` (unlit colour, no edges) or `wireframe`. `overlays` is any of `grid`, `axes`, `labels`, `bounds`, `selection` (tints the session selection; `highlight` wins when both name an object). `highlight` is object ids: tinted, with bright edges drawn through occluders. Result: `{width, height, camera, labels?}`; `labels` (only with the `labels` overlay) lists `{id, x, y}` in pixels for every brush, mesh and point entity whose bounds centre is inside the image. A label does not mean the object is unoccluded. Unrenderable objects add a warning. The CLI writes the PNG and adds `path` (session mode takes a `path` argument, or returns `png_base64`); MCP returns it as image content.
- `view.camera.list` (read) → `{cameras, viewports}`.
- `view.camera.get {camera}` (read) → `{camera}`. Session cameras come back framed on the scene for 1024 x 768, with `id: null`.
- `view.camera.create {name, position, yaw?, pitch?, target?, fov?}` or `{name, ortho, position?, zoom?}` (mutate). `target` sets yaw and pitch (not with yaw/pitch). An ortho camera without position or zoom is framed on the scene for 1024 x 768. Names must be unique, non-empty, not start with `viewport.` and not look like an id. `fov` is in (1, 179), `pitch` within ±89, `zoom` > 0.
- `view.camera.set {camera, name?, position?, yaw?, pitch?, target?, fov?, ortho?, zoom?}` (mutate). `ortho` switches to orthographic, `fov` back to perspective. Session cameras cannot be edited.
- `view.camera.delete {cameras}` (destructive). Cameras only.
- `view.frame {ids, camera, width?, height?}` (mutate). Moves a document camera (and sets the zoom of an ortho one) so the objects fill the image at the given size (default 1024 x 768). Orientation and fov are kept.

Camera edits are ordinary undoable commands.

### `check.validate {ids?}` (T0)
Returns issues `{code, severity, ids, message, hint}`. Tier 0 codes: `invalid_brush`, `invalid_mesh`, `degenerate_face`, `mesh_open`, `mesh_nonmanifold`, `mesh_nonplanar_face`, `empty_brush_entity`, `missing_target`, `duplicate_object`. Leak checks wait for a compiler (T3).

## Semantics

- **Transactions:** a batch or `begin`/`commit` pair applies atomically and undoes as one step. GUI drags use the same mechanism, with consecutive-edit merging.
- **Concurrency:** the editor process is the single writer. A revision counter increments per committed transaction; `expect_revision` rejects stale writes with `RevisionMismatch`.
- **Events:** every committed change is broadcast to all connected clients.
- **Attribution:** each transaction records its originating client, shown in `history.list`.
- **Dry run:** returns the would-be changeset without committing.
- **Undo storage:** each history entry stores before/after snapshots of every object it touched, so undo and redo work the same for every command. Undo never rewinds the ID counter.
- **File mode:** `level-cli` loads the file, runs the command in a fresh session and saves, so revision and undo history do not carry from one invocation to the next. A long-lived editor session keeps them.
- **Locks:** locked-object behaviour arrives with locking in T1.

## CLI mapping

```
level-cli ops [--safety read|mutate|destructive] [--json]
level-cli schema <op>
level-cli <file> new [--name N] [--force]
level-cli <file> run <op> --args '{...}' [--dry-run] [--expect-revision N]
level-cli <file> apply batch.json [--dry-run]
level-cli <file> screenshot --camera top -o out.png
level-cli <file> session [--autosave] [--read-only]
```

The response JSON goes to stdout (`--pretty` to indent). Exit code 0 on success, 1 for a command error, 2 for an I/O, file-format, usage or render (`Render`) error. The file is rewritten atomically, and only when a non-dry-run command changed something.

`ops` lists the registry (name, safety, summary; `--json` adds scope and undoable). `schema` prints an op's argument schema. Neither needs a file.

`run` and `apply` are one-shot: they load the file, run in a fresh session and save, so ops that keep state between commands (`history.*`, `select.*`, `file.new`, `file.open`) are refused with `InvalidArgument` and a hint to use `session` or MCP. `file.info` and `file.save` work.

`session` loads the file once and keeps one in-memory `Session` and one renderer:
- Each non-empty stdin line is a request envelope (`{"op", "args"?, "dry_run"?, "expect_revision"?}`); each gets exactly one response line on stdout. Bad JSON and command errors are response lines, not exits.
- Undo, transactions, selection, revision and dirty state work as in MCP. Nothing is written until `file.save` (or after every change with `--autosave`). EOF with unsaved changes prints a warning on stderr. `--read-only` refuses mutate and destructive ops (`NotPermitted`).
- `view.screenshot` takes an extra `path`; the PNG is written there. Without it the result carries `png_base64`.

`screenshot` flags: `--camera` (default `viewport.persp`), `-o/--out` (required; written atomically), `--width`, `--height`, `--mode`, `--overlay grid,axes,labels,bounds,selection`, `--highlight b1,b2`, and `--args '{...}'` for the full `view.screenshot` arguments (flags override it). It never modifies the level file and has no selection (it is a one-shot). The `Render` exit code is also used when no graphics adapter exists, so a headless machine needs a Vulkan driver (see the plan, risk 5).
