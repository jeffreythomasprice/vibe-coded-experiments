# Scene format (TOML)

Plain-text, git-diffable, versioned. See [model](tier0-model.md) for what the records mean.

## Layout

Extension `.scene.toml`. A single file in Tier 0.

1. `format = "scene"`, `version = 3`
2. `[meta]` — `name`, `next_id`
3. `[world]` — world properties
4. `[editor]` — grid size and other editor metadata
5. Arrays of tables in fixed order, each sorted by numeric ID: `[[material]]`, `[[component_defs]]`, `[[layer]]`, `[[group]]`, `[[camera]]`, `[[entity]]`, `[[brush]]`, `[[mesh]]`

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

Face keys: `id`, `plane` (brush) or `verts` (mesh), `material`, then optional `offset`, `scale`, `rotation`, `u_axis`, `v_axis`. The axes come as a pair; one without the other is a load error. Unknown keys inside a face or vertex table are a load error (only whole records keep unknown keys).

## Materials

```toml
[[material]]
id = "t7"
name = "base/brick"
texture = { path = "../tex/brick.png" }        # or an absolute path, or { format = "png", base64 = "iVBOR…" }
color = [1, 0.9, 0.8, 1]
emissive = [0.25, 0, 0]
emissive_strength = 2
roughness = 0.5
metallic = 0.25
filter = "nearest"
double_sided = true
unlit = true
nodraw = true
```

Keys, in this order: `id`, `name`, `texture`, `color`, `emissive`, `emissive_strength`, `roughness`, `metallic`, `filter`, `double_sided`, `unlit`, `nodraw`; defaults are omitted (`color = [1,1,1,1]`, no emission, `roughness = 1`, `metallic = 0`, `filter = "linear"`, flags false). A relative `texture.path` is relative to the directory of the scene file; embedded `base64` is the image file's bytes (formats `png`, `jpeg`, `tga`, `bmp`). A face refers to a material by `name`; a name no `[[material]]` defines still loads. Names are unique (`DuplicateMaterialName`). Unknown keys in a material record are kept, like other records. See [materials.md](materials.md).

## Component definitions

```toml
[[component_defs]]
id = "d20"
path = "defs/game.components.json"     # absolute, or relative to the scene file

[[component_defs]]
id = "d21"
json = '''
{ "version": 2, "components": [ ... ] }
'''                                  # embedded: the definitions file, pretty-printed
```

Exactly one of `path` and `json` per record. The embedded text is the definitions file, pretty-printed with sorted keys (`components` before `version`), in a multi-line literal string (a basic string if it contains `'''`). The file format is `schemas/component-defs.schema.json`. A path that does not exist, or a file that is not a definitions file, does not stop the scene from loading; `check.validate` reports it. Embedded JSON that does not match the schema is a load error carrying the JSON pointer. See [entities.md](entities.md).

## Cameras, components and world properties

- Perspective camera: `position`, `yaw`, `pitch` (each omitted when 0), `fov`.
- Orthographic camera: `position`, `yaw`, `pitch`, `ortho = "top" | "front" | "side"`, `zoom`.
- Angle, axis and zoom conventions are in [tier0-model.md](tier0-model.md#camera).
- An entity record is `id`, `layer`, `group?`, flags, `origin` (always), `angles` (omitted when zero), then `components`: a single inline table of component name to data, e.g. `components = { light = { intensity = 300 }, player_start = {} }`, omitted when empty. A component's data and `[world]` hold any JSON object, written as inline TOML: nested tables, arrays of any depth, strings, bools, integers and floats (a whole-number float is written `1.0`, so integer/float round-trips). Keys are written sorted. `null` and dates are not supported (a date is a load error). See [entities.md](entities.md).

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

Load as `toml::Table`, then deserialize into typed structs. Always write the latest version. Refuse files newer than supported (`UnsupportedVersion`) or older than the oldest readable (`VersionTooOld`).

- **Version 2** added `[[material]]` records and face `u_axis`/`v_axis`.
- **Version 3** replaced the entity `classname` and `properties` with `components`, made `origin` required on every entity (a brush entity's origin is its pivot), and replaced `[[entity_defs]]` with `[[component_defs]]` (definitions files are `*.components.json`, version 2). There is no migration: versions below 3 are refused with `VersionTooOld`, and the scenes in this repository were rewritten by hand.

## Loader errors

A specific enum with the offending record ID: `Parse`, `UnsupportedVersion`, `DuplicateId` (also for a repeated face or vertex ID, reported as `b8/f3`), `DuplicateMaterialName`, `DanglingReference`, `GroupCycle`, `InvalidGeometry` (a brush that is not bounded or has degenerate planes; a mesh with error-level issues; mesh warnings do not fail a load). Loading never rewrites geometry.

## Worked example

A small room, an animated door entity, a light, a player start, a donut-style mesh and a camera (abbreviated: the room uses two walls and the mesh a single quad).

```toml
format = "scene"
version = 3

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
layer = "l1"
origin = [64, 64, 16]
angles = [0, 90, 0]
components = { player_start = {} }

[[entity]]
id = "e6"
layer = "l1"
origin = [128, 128, 112]
components = { light = { color = [1, 0.9, 0.8], intensity = 300 } }

[[entity]]
id = "e7"
layer = "l1"
origin = [96, 0, 0]
components = { door = { locked = false }, keyframe_animation = { keyframes = [{ time = 0 }, { angles = [0, 90, 0], time = 1.5 }] } }

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
