# The simulation and its ECS

The editor shows a scene as it is authored. The **simulation** shows it as it is played: entities carry data that drives behaviour over time, and a separate client (`editor-game`, see [game.md](game.md)) draws one camera of the result. This document describes `editor-sim`, the crate that owns the simulation. It is GPU-free and depends only on `editor-core`, `editor-geom` and `editor-schema`.

Nothing here changes the document. The editor never runs the simulation: a keyframe animation does not move anything in the editor, only in the game client and in `view.screenshot` with `simulate`.

## What the document turns into

`Sim::load(&Document)` builds a `World` once and returns `LoadWarning`s for anything it had to skip.

| Document | Simulation |
|---|---|
| Entity | one ECS entity with `Source`, `RestPose` and `CurrentPose`, plus one ECS component per entity component |
| Brush or mesh owned by an entity | listed in that entity's `Members` |
| Brush or mesh owned by no entity | `StaticGeometry` (a resource): it never moves |
| Groups, layers, cameras, materials | not simulated (layers only matter through `omit_from_export`, below) |

- Hidden flags are an editing aid and are ignored: a hidden object still exists in the simulation and is drawn by the game client.
- Objects on a layer with `omit_from_export` are left out entirely. `Sim::excluded()` lists them so a renderer can skip them.
- Entities are loaded in id order, so the lowest-id entity is the lowest ECS index.

## The ECS

A small hand-written ECS (`crates/editor-sim/src/world.rs`).

- `Entity { index, generation }`. `World::spawn`, `despawn` and `is_alive`. A despawned slot is reused with a higher generation, so an old handle never reaches the new entity.
- Components are any `'static` type. `World::insert`, `get`, `get_mut`, `remove`, and `iter::<T>()` over every live entity that has a `T`. Storage is one dense `Vec<Option<T>>` per type, indexed by entity index.
- Resources are typed singletons: `insert_resource`, `resource`, `resource_mut`.
- A system is `fn(&mut World)`. A `Schedule` runs its systems in the order they were added. There are no queries over several components at once: a system iterates one component and looks the others up with `get`. Add a richer query API when a second system needs it.

Resources that exist after `load`: `Time { elapsed, dt }` (seconds) and `StaticGeometry { objects }`.

### Components the simulation defines

| Component | Meaning |
|---|---|
| `Source(EntityId)` | the document id this entity was built from |
| `RestPose(Pose)` | the entity's origin and angles as authored: the implied transform |
| `CurrentPose(Pose)` | where it is now; equal to `RestPose` until a system moves it |
| `Members(Vec<ObjectId>)` | the brushes and meshes the entity owns |
| `Track` | a sorted, ready-to-sample copy of `keyframe_animation` |
| `KeyframeAnimation`, `PlayerStart` | the typed data of the built-in components, see [components.md](components.md) |
| `Dynamic(BTreeMap<String, Value>)` | every entity component the simulation does not interpret, by name, as the raw JSON the editor stores |

`Pose` is `{ translation: DVec3, rotation: DQuat }`. A rest pose is built from `origin` and `angles` with `editor_geom::euler::orientation` (`Rz(yaw) * Ry(-pitch) * Rx(roll)`, the same convention the editor uses everywhere).

### Systems

Run once per `step` and once on `load` (so a pose exists before the first frame), in this order:

1. `animate`: for each entity with a `Track`, sample it at `Time.elapsed` and write `CurrentPose`. See [components.md](components.md#keyframe_animation).

## Time

- `Sim::step(dt)` adds `dt` seconds and runs the schedule. The game client clamps `dt` to 0.1 s so a stall does not teleport movers.
- `Sim::set_time(t)` sets the elapsed time directly and runs the schedule. `view.screenshot` with `simulate: {time}` uses it.
- Reloading after an edit (the game client does this on every change event) builds a fresh `Sim` and restores the elapsed time, so animations continue instead of restarting.

## Reading the result

- `Sim::poses() -> HashMap<EntityId, DAffine3>`: for each entity that owns geometry, the transform from its rest pose to its current pose, `current * rest.inverse()`. The renderer applies it to that entity's members (`Renderer::draw_viewport_posed`).
- `Sim::spawn() -> Option<Pose>`: the rest pose of the lowest-id entity with `player_start`.
- `Sim::static_geometry()`, `Sim::excluded()`, `Sim::entity(EntityId)`, `Sim::world()`.

## Adding a component or a system

1. Add the component's data type to `schemas/components.schema.json` (closed objects, defaults for optional fields; see [schemas.md](schemas.md)) and list its definition name in `crates/editor-sim/build.rs`.
2. Add a row to `BUILTINS` in `crates/editor-cmd/src/component_defs.rs` so the editor knows its name, category and schema.
3. Parse it in `Sim::load` (`crates/editor-sim/src/sim.rs`) with `editor_schema::parse`, and insert the typed value on the ECS entity. A parse failure is a `LoadWarning::BadComponent`, never a panic.
4. Write the system as a `fn(&mut World)`, add it to the schedule in `Sim::load`, and test it in `crates/editor-sim/tests/`.
5. Document the component in [components.md](components.md).

A component that is only data for a game built on top of this (no built-in behaviour) needs none of that: define it in a `*.components.json` file ([entities.md](entities.md)) and read it from `Dynamic`.

## Not here yet

Triggers and events between entities, several animations per entity, animation started by an event, collision and physics. `editor.linkType` links are already stored and resolved by the editor; the simulation does not read them yet.
