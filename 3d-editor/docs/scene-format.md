# Scene format (TOML)

Plain-text, git-diffable, versioned. See [model](tier0-model.md) for what the records mean.

## Layout

Extension `.scene.toml`. A single file in Tier 0.

1. `format = "scene"`, `version = 1`
2. `[meta]` — `name`, `next_id`
3. `[world]` — world properties
4. `[editor]` — grid size and other editor metadata
5. Arrays of tables in fixed order, each sorted by numeric ID: `[[layer]]`, `[[group]]`, `[[camera]]`, `[[entity]]`, `[[brush]]`, `[[mesh]]`

Layers, groups, entities, brushes and meshes take optional `hidden = true` and `locked = true` (written only when true; they apply to everything the object contains), and a layer also `omit_from_export = true`. A file without them loads unchanged, so these did not bump `version`.

Children point at parents (`layer`, `group`, `entity`); records never list children. A nested group names its parent group with `group`, like any other object. Each record is independent, so a later split into `scene.toml` plus `layers/<name>.toml` is a pure re-partition.

## Brushes and meshes

Faces and mesh vertices are multi-line arrays of single-line inline tables, one element per line, so diffs are per face (valid TOML 1.0).

```toml
[[brush]]
id = "b12"
layer = "l1"
faces = [
  { id = 1, plane = [[0, 0, 64], [0, 64, 64], [64, 0, 64]], material = "base/floor" },
  { id = 2, plane = [[0, 0, 0], [0, 64, 0], [0, 0, 64]], material = "base/wall", offset = [8, 0] },
]

[[mesh]]
id = "m3"
layer = "l1"
vertices = [
  { id = 1, p = [0, 0, 0] },
  { id = 2, p = [64, 0, 0] },
  { id = 3, p = [64, 64, 0] },
  { id = 4, p = [0, 64, 0] },
]
faces = [
  { id = 1, verts = [1, 2, 3, 4], material = "base/trim" },
]
```

Plane points use the Quake/TrenchBroom convention: clockwise seen from outside, normal = (c − a) × (b − a), so the first example point set `[[0,0,64],[0,64,64],[64,0,64]]` is a top face.

Face keys: `id`, `plane` (brush) or `verts` (mesh), `material`, then optional `offset`, `scale`, `rotation`. Unknown keys inside a face or vertex table are a load error (only whole records keep unknown keys).

## Cameras and properties

- Perspective camera: `position`, `yaw`, `pitch` (each omitted when 0), `fov`.
- Orthographic camera: `position`, `yaw`, `pitch`, `ortho = "top" | "front" | "side"`, `zoom`.
- Angle, axis and zoom conventions are in [tier0-model.md](tier0-model.md#camera).
- Entity `properties` and `[world]` values are strings, numbers, bools or `[x, y, z]` vec3. Keys are written sorted. A color is just a vec3 until entity definitions (Tier 1) say otherwise.

## Canonical writer

- Fixed key order per record; defaults omitted.
- Integral values written as integers; other floats as shortest round-trip; `-0` normalized to `0`.
- Output is byte-stable for an unchanged document.
- Unknown keys are preserved per record (flattened extra table) and written after known keys, sorted.
- Strings use basic `"..."` quoting; keys are bare when they match `[A-Za-z0-9_-]+`.

## IDs

- Global `next_id` in `[meta]`; IDs never reused. Every record's number is unique across all kinds (`l1` and `c1` cannot both exist). If a record's number is at or above `next_id`, the loader raises `next_id` past it.
- Local face and vertex IDs: next local ID is max+1 on load (reuse of deleted local IDs across a reload is acceptable).

## Versioning

Load as `toml::Table`, apply migrations v(N)→v(N+1), then deserialize into typed structs. Always write the latest version. Refuse files newer than supported.

## Loader errors

A specific enum with the offending record ID: `Parse`, `UnsupportedVersion`, `DuplicateId` (also for a repeated face or vertex ID, reported as `b8/f3`), `DanglingReference`, `GroupCycle`, `InvalidGeometry` (a brush that is not bounded or has degenerate planes; a mesh with error-level issues; mesh warnings do not fail a load). Loading never rewrites geometry.

## Worked example

A small room, a door brush entity, a light, a player start, a donut-style mesh and a camera (abbreviated: the room uses two walls and the mesh a single quad).

```toml
format = "scene"
version = 1

[meta]
name = "example"
next_id = 20

[world]
sky = "sky/overcast"

[editor]
grid = 16

[[layer]]
id = "l1"
name = "main"

[[camera]]
id = "c2"
name = "overview"
position = [-256, -256, 256]
yaw = 45
pitch = -30
fov = 90

[[entity]]
id = "e5"
classname = "info_player_start"
layer = "l1"
origin = [64, 64, 16]
angles = [0, 90, 0]

[[entity]]
id = "e6"
classname = "light"
layer = "l1"
origin = [128, 128, 112]
properties = { color = [1, 0.9, 0.8], intensity = 300 }

[[entity]]
id = "e7"
classname = "func_door"
layer = "l1"
properties = { speed = 100, targetname = "door1" }

[[brush]]
id = "b8"
layer = "l1"
faces = [
  { id = 1, plane = [[0, 0, 0], [0, 256, 0], [256, 0, 0]], material = "base/floor" },
  { id = 2, plane = [[0, 0, -16], [256, 0, -16], [0, 256, -16]], material = "base/floor" },
  { id = 3, plane = [[0, 0, 0], [0, 256, 0], [0, 0, 1]], material = "base/wall" },
  { id = 4, plane = [[256, 0, 0], [256, 0, 1], [256, 256, 0]], material = "base/wall" },
  { id = 5, plane = [[0, 0, 0], [0, 0, 1], [256, 0, 0]], material = "base/wall" },
  { id = 6, plane = [[0, 256, 0], [256, 256, 0], [0, 256, 1]], material = "base/wall" },
]

[[brush]]
id = "b9"
layer = "l1"
entity = "e7"
faces = [
  { id = 1, plane = [[96, 0, 0], [96, 16, 0], [96, 0, 96]], material = "base/door" },
  { id = 2, plane = [[128, 0, 0], [128, 0, 96], [128, 16, 0]], material = "base/door" },
  { id = 3, plane = [[96, 0, 0], [96, 0, 96], [128, 0, 0]], material = "base/door" },
  { id = 4, plane = [[96, 16, 0], [128, 16, 0], [96, 16, 96]], material = "base/door" },
  { id = 5, plane = [[96, 0, 0], [128, 0, 0], [96, 16, 0]], material = "base/door" },
  { id = 6, plane = [[96, 0, 96], [96, 16, 96], [128, 0, 96]], material = "base/door" },
]

[[mesh]]
id = "m10"
layer = "l1"
vertices = [
  { id = 1, p = [0, 0, 0] },
  { id = 2, p = [32, 0, 0] },
  { id = 3, p = [32, 32, 0] },
  { id = 4, p = [0, 32, 0] },
]
faces = [
  { id = 1, verts = [1, 2, 3, 4], material = "base/trim" },
]
```

## Hand-editing notes

An agent can edit the file with ordinary file tools: change property values, materials, and vertex positions safely. After editing, run `check.validate`. Do not reuse IDs, and bump `next_id` when adding records.
