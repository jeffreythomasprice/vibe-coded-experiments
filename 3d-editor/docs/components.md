# Components

An entity is a pose plus a set of named **components**. This page documents the transform every entity has and each built-in component. Keep it current: whenever a component is added or changed (built-in, or one of the shipped definitions in `scenes/defs/`), update its section here. The machinery (definitions files, validation, the inspector) is in [entities.md](entities.md); how the simulation consumes them is in [ecs.md](ecs.md).

## The transform (implied, on every entity)

Not a component: it is stored on the entity itself and edited with the normal move and rotate controls, `entity.set_transform` and `transform.*`, never in the component editor.

- `origin`: `[x, y, z]`, world units. Always present.
- `angles`: `[pitch, yaw, roll]` in degrees, default `[0, 0, 0]`. Orientation is `Rz(yaw) * Ry(-pitch) * Rx(roll)`: zero angles face +X, positive pitch looks up, yaw 90 faces +Y.
- For an entity that owns brushes and meshes the origin is the **pivot**. Its members stay in world space in the file; moving the entity moves them with it unless `entity.set_transform` is given `members: "keep"`.

## Built-in components

Built-ins are known to the editor without any definitions file, are validated by their schema (warnings, never errors), and cannot be redefined by a definitions file (`duplicate_component`). Their data types are in `schemas/components.schema.json`.

### `keyframe_animation`

Moves its entity through a list of keyframes, forever, in the simulation. In the editor the entity stays at its authored pose.

```toml
components = { keyframe_animation = { interpolation = "smooth", keyframes = [
  { time = 0, angles = [0, 0, 0] },
  { time = 1.5, angles = [0, 90, 0] },
  { time = 3.5, angles = [0, 90, 0] },
  { time = 5, angles = [0, 0, 0] },
] } }
```

| Field | Type | Meaning |
|---|---|---|
| `keyframes` | array, at least 1 | the animation (default when added from the inspector: one keyframe at time 0) |
| `keyframes[].time` | number >= 0 | seconds from the start of the loop |
| `keyframes[].offset` | `[x, y, z]`, default `[0, 0, 0]` | translation from the entity's origin, world units |
| `keyframes[].angles` | `[pitch, yaw, roll]`, default `[0, 0, 0]` | degrees added to the entity's orientation |
| `interpolation` | `"linear"` (default) or `"smooth"` | `smooth` eases in and out between keyframes (smoothstep `f*f*(3-2f)`) |

Semantics, exactly (`crates/editor-sim/src/anim.rs`):

- Keyframes are sorted by `time`; the order in the file does not matter.
- The loop length is the largest `time`. Time wraps: `t' = elapsed mod length`, so the animation restarts as soon as it ends. To hold a pose before looping, add a keyframe that repeats it.
- Before the first keyframe (when its `time` is above 0) the first keyframe's pose is held.
- A single keyframe, or a loop length of 0, is a static pose.
- Between two keyframes `offset` and `angles` are interpolated component by component. Angles are plain degrees and are not wrapped, so `[0, 360, 0]` is a full spin and `[0, 90, 0]` to `[0, -90, 0]` turns the long way through 0, not through 180.
- The pose at a time is: translation = `origin + offset`; rotation = `R(angles) * R(rest angles)`. The rotation is about the entity's origin and in world axes, so putting an entity's origin at a hinge makes its members swing about the hinge.
- Offsets and angles are relative to the authored pose, so a keyframe of all zeros is the pose seen in the editor.
- The brushes and meshes the entity owns move rigidly with it. Lighting is baked into the vertex colours at load, so a rotating mover keeps the shading it had at rest.

Schema violations (for example an empty `keyframes`) are reported by `check.validate` as `schema_violation`; the simulation skips a component that does not parse, with a `LoadWarning`.

Not yet: triggering by an event, several named animations per entity, one-shot or ping-pong playback, per-keyframe easing.

### `player_start`

Marks where the game client puts the player. No fields (`{}`).

- The player is placed at the entity's origin raised by 12 units (eye height above the marker centre) and faces the entity's yaw and pitch.
- With several, the lowest entity id is used. With none, the game client starts from the same camera as `viewport.persp`.
- Editor marker: a green box, 12 x 12 x 32.

## Components from definitions files

Anything else is a user component. A definitions file (`*.components.json`) gives it a name, an optional category, description and marker colour or size, and a JSON Schema for its data; see [entities.md](entities.md). The simulation keeps these as raw JSON in the `Dynamic` component of the ECS entity.

`scenes/defs/example.components.json` ships these. They have no built-in behaviour yet: they exist as editor support and as data for a game built on the simulation.

| Component | Fields | Intended use |
|---|---|---|
| `light` | `color` `[r, g, b]` 0..1 (default white), `intensity` (default 300), `radius` (default 512), `flicker` (default false) | a point light; drawn as an octahedron |
| `sound_source` | `sound` (required), `volume` 0..1, `radius`, `loop` | a positional sound |
| `relay` | `event`, `targets` (entity links) | forwards an event |
| `button` | `event`, `targets` (entity links) | sends an event when pressed |
| `trigger_zone` | `on_enter`, `on_exit`: lists of `{target (link), event}` | reports entering and leaving a region |
| `door` | `locked` (default false), `sound` | door state; its motion comes from `keyframe_animation` |
| `elevator` | `stops` (`{name?, offset, wait}`), `speed` | a platform between stops |
