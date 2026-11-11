# Tier 0 — Scene model

What exists in a scene and how it is addressed. Companion docs: [commands](tier0-commands.md), [file format](scene-format.md), [overall plan](plan.md).

## Space and units

- Z-up, right-handed. Units are abstract; default grid size is 16.
- Ortho views: **Top** = XY looking −Z, **Front** = XZ looking +Y, **Side** = YZ looking −X.
- The document stores `f64`; the renderer converts to `f32`. Angles are degrees.

## IDs

One document-wide counter, never reused, serialized with a kind prefix:

| Prefix | Kind |
|---|---|
| `b` | brush |
| `m` | mesh |
| `e` | entity |
| `g` | group |
| `l` | layer |
| `c` | camera |

Sub-elements have object-local numeric IDs, addressed with a path:

- Brush or mesh face: `b12/f3`, `m3/f2`
- Mesh vertex: `m3/v10`
- Mesh edge: derived, addressed by vertex pair `m3/v1-v2`

IDs survive save/load and undo, so an agent's references stay valid.

## Element references

A face or vertex is addressed from an object with a path, parsed by `editor_core::ElementRef`:

- `b12/f3`, `m3/f2`: a face. `m3/v10`: a mesh vertex.
- A bare object id (`b12`) means every face (face operations) or every vertex (vertex operations).
- Local ids are per object and never reused within it; an unknown one is `NotFound` with the full path as the target.

## Hierarchy

document → layers → (nested groups) → objects.

- Every object except layers and cameras belongs to exactly one layer, including objects inside groups and brush entities.
- Ownership is a parent pointer on the child (`layer`, `group`, `entity`), never a child list.
- Groups carry no transform in Tier 0 (geometry is world-space, as in TrenchBroom). Linked groups add a transform in Tier 2.

### Hidden and locked

Brushes, meshes, entities, groups and layers (not cameras) carry two flags, `hidden` and `locked`, saved in the scene file and undoable like any other edit. A flag applies to everything the object contains, so an object is *effectively* hidden or locked when it or any container is: its brush entity, its group chain, or its layer (`Document::effective_flags`, `hidden_ids`, `locked_ids`).

- Hidden objects are not drawn, picked or raycast, and cannot be selected.
- Locked objects refuse edits, deletion and additions: see [locks](tier0-commands.md#locks).
- A layer also has `omit_from_export`, stored and round-tripped; nothing exports yet.
- Group open/close (what a click selects) is GUI state, not document state: see [gui.md](gui.md).

## Brush (convex)

Planes are the source of truth; vertices and edges are derived by `editor-geom`.

- At least 4 faces.
- Each face: a plane given by 3 points (clockwise seen from outside, the Quake convention: normal = (c − a) × (b − a); grid-aligned planes stay exact), a material, and UV attributes (offset, scale, rotation; projection axis chosen from the normal).
- Valid means: bounded, non-empty, no zero-area faces. Redundant planes are dropped on normalization.
- Vertices have no ids, because every edit to the planes can reorder them. Commands that move them (`vertex.translate`) address a corner by its position.

## Mesh (polygon mesh, possibly concave)

Vertices are the source of truth. This is the kind that holds concave results such as a donut made by subtracting one cylinder from another, and whose faces and vertices can then be moved individually.

- Vertices have local IDs and positions.
- Faces are ordered vertex-ID loops (n-gons, counter-clockwise from outside, simple polygons, no holes — a donut is many faces) plus the same material/UV attributes as brush faces.
- May be concave.
- Closed-manifold is expected, but open or non-manifold meshes are validation warnings, not errors.
- Non-planar faces are a warning and render triangulated.

## Conversions and CSG

- brush → mesh: always, lossless.
- mesh → brush: only if closed and convex; otherwise a structured error. Convex decomposition is Tier 2.
- CSG (Tier 1): brush-only operations default to brush output (classic convex fragments). `output = "mesh"` welds the result into one mesh. Any mesh operand forces mesh output.
- Implementation approach: plane-based clipping for brushes, BSP polygon CSG for meshes, results snapped to the grid.
- Mesh CSG triangulates its inputs, runs the BSP, then snaps, welds, repairs T-junctions and re-validates. If the result is not watertight it returns `CsgError::InvalidResult` instead of a broken mesh. Faces come out fragmented (convex pieces); merging coplanar fragments is not done yet.

## Tolerance

`editor-geom` uses epsilon-tolerant `f64`, not exact arithmetic. All constants live in `editor-geom/src/tolerance.rs` and are ordered so each scale is well below the next: snap grid (2⁻²⁰) ≪ plane-side eps (1e-5 brushes, 1e-4 CSG) ≪ weld radius (1e-3) ≈ planarity (2e-3). Features smaller than the weld radius collapse. Coordinates must stay well inside ±2¹⁹.

Problems are reported, never panicked on: brushes return `BrushError`, meshes are checked by `validate` (errors vs warnings per the rules above), and CSG returns `CsgError`.

Measured on random slanted brushes (cuboids cut by up to 4 random integer planes): no single operation in 10,000 pairs, and about 2 in 10,000 three-operand chains, end in `InvalidResult`. Axis-aligned, grid-aligned and regular-prism cases (including the donut) are exact. Property tests run every property under 8 fixed seeds; `EDITOR_GEOM_SEEDS` and `EDITOR_GEOM_SEED_BASE` widen or shift the sweep, and a failure message names the seed to replay. See `crates/editor-geom/tests/`.

## Entity

- `classname` plus typed properties: string, number, bool, vec3. (Color is a vec3 until entity definitions arrive in Tier 1; the file cannot tell them apart.)
- Point entity: `origin` and `angles` (pitch, yaw, roll).
- Brush entity: owns brushes and meshes (children point at it).
- Classnames are free-form in Tier 0; definitions arrive in Tier 1.
- World geometry is any brush or mesh with no owning entity. World properties live on the document.

## Material

A name/path string on each face (e.g. `"base/concrete_01"`). Tier 0 renders a flat colour hashed from the name: FNV-1a 64 of the UTF-8 bytes, hue = hash mod 360, saturation 0.5, lightness 0.45 to 0.65 from bits 16 to 23. The empty name is grey. Point entities draw as 16-unit boxes coloured the same way from the classname.

## Camera

Named cameras saved in the document: position, yaw/pitch, fov — or orthographic (view axis, zoom). The four GUI viewport cameras are session state, addressable as `viewport.persp`, `viewport.top`, `viewport.front`, `viewport.side`. The math lives in `editor_geom::camera`.

- **Perspective:** `position` is the eye. `yaw` is degrees about +Z (0 looks along +X, 90 along +Y); `pitch` is degrees, positive looks up, within ±89. `fov` is the vertical field of view in degrees.
- **Orthographic:** the `ortho` axis fixes the orientation and `yaw`/`pitch` are ignored. Top looks −Z with +X right and +Y up; Front looks +Y with +X right and +Z up; Side looks −X with +Y right and +Z up. `position` is the view centre (its component along the view axis is ignored). `zoom` is pixels per world unit, so the visible area depends on the image size.
- **Session cameras** do not exist in the file. Each is computed on request, framed on the scene bounds (an empty scene frames ±64) with a 10% margin. `viewport.persp` looks from yaw 45, pitch −30, fov 60.
- Images have row 0 at the top. Ortho and perspective share the same projection code for screenshots and, later, the GUI viewports.

## Selection

One shared editor selection. It is session state: not saved, not in undo history. An agent can read what the human selected and change it to point things out. Brushes, meshes, entities and groups can be selected (layers and cameras cannot). The session prunes the selection whenever objects disappear: after a delete, an undo or redo, or loading another file.

## Transform conventions

- Rotation follows the right-hand rule: positive degrees about +Z turn +X toward +Y.
- A mirroring transform (negative determinant) keeps solids valid: a brush's plane points swap so normals stay outward, a mesh's face loops reverse.
- Brush plane points and mesh vertices are snapped back onto the geometry grid after any transform; the brush is then re-normalized, so an invalid result is an error rather than a broken brush.
- Entity `angles` are `[pitch, yaw, roll]` in degrees. The orientation is `Rz(yaw) · Ry(−pitch) · Rx(roll)`: with zero angles an entity faces +X, positive pitch looks up, yaw 90 faces +Y. A pure rotation recomposes the angles; scales and mirrors leave them alone.
- Groups carry no transform: transforming a group transforms its members.
