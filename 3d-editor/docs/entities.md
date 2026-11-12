# Entities, components, definitions and links

An entity is a pose (`origin`, `angles`) plus a set of named **components**. A component is an open JSON object; its name says what it means. There are no classnames: what an entity *is* follows from the components it carries. An entity may also own brushes and meshes (a brush entity); that is just an entity that has members, so a door leaf is one entity with a `door` component, a `keyframe_animation` component and one brush.

The editor does not interpret component data. A component definition is optional and describes it with a JSON Schema; one schema hook marks a string as a reference to another entity. The simulation ([ecs.md](ecs.md)) interprets the built-in components ([components.md](components.md) lists every one).

Everything here is advisory. Nothing is ever refused because of a schema, and loading a scene never fails because of one. Problems are warnings on the op that caused them and issues from `check.validate`.

## Entity

- `origin`: always present. `angles`: `[pitch, yaw, roll]`, default zero. Together they are the implied transform ([components.md](components.md#the-transform-implied-on-every-entity)); they are not a component and are never edited in the component editor.
- `components`: component name to data object, names unique, `[A-Za-z0-9_]+`. Stored sorted. `null` is rejected anywhere inside (TOML cannot store it; the error names the JSON pointer). Numbers keep their integer/float distinction through save and load, and keys are written sorted.
- Members: brushes and meshes whose `entity` is this entity. An entity with no members is a **point entity** (the viewports draw a marker for it); with members it is a **brush entity** (no marker; the members are drawn; the origin is a pivot, shown as a small cross when selected).
- There are no entity names. Entities are addressed by id (`e7`).

## Definitions

A definitions file is JSON (`*.components.json`):

```json
{
  "version": 2,
  "components": [
    {
      "name": "button",
      "category": "logic",
      "description": "Pressing it sends an event to its targets.",
      "color": [0.9, 0.55, 0.2],
      "schema": {
        "type": "object",
        "properties": {
          "event": { "type": "string", "default": "press" },
          "targets": { "type": "array", "items": { "type": "string", "editor": { "linkType": "entityID" } } }
        }
      }
    }
  ]
}
```

- `name` is the component name.
- `color` (0..1 per channel) and `size` (`{min, max}` relative to the origin) set the marker the viewport draws for a point entity carrying the component. An entity is drawn with the look of its first component (in name order) that has a colour or size; otherwise with a colour hashed from its first component's name, or grey with no components. A component whose name starts with `light` and has no size is drawn as an octahedron.
- `schema` is a JSON Schema (draft 2020-12 unless `$schema` says otherwise) for the data object. Unknown keywords such as `editor` are ignored by validation. Top-level `properties.*.default` values fill the data when `entity.add_component` gets no `data`.
- `category` and `description` feed the component browser and the inspector.

The file format itself is defined by `schemas/component-defs.schema.json` (see [schemas.md](schemas.md)); the Rust types are generated from it and a file is read with `editor_schema::parse_slice::<ComponentDefsFile>`, so a malformed file reports JSON pointers (`/components/0/name`). Embedded definitions in a scene are written with sorted keys. `version` is 2.

`scenes/defs/example.components.json` has components for lights, sounds, relays, buttons, trigger zones, doors and elevators.

### Built-in components

`keyframe_animation` and `player_start` need no definitions file: the catalog always contains them, with the schema from `schemas/components.schema.json` (category `movers` and `player`). A definitions file that redefines one gets a `duplicate_component` problem and the built-in wins.

### Sources

A scene holds any number of definitions records (`d42`, an object kind like materials), each with one source, like a material's texture:

- a file at an absolute path,
- a file relative to the scene file (the default once the scene is saved), or
- embedded in the scene.

Later records override earlier ones for the same component name (`duplicate_component` warning). `component_defs.add` (`store: absolute|relative|embed` for files), `component_defs.embed` and `component_defs.remove` manage records; `component_defs.list` returns the records, the resolved components (built-ins first, `from: null`) with their schemas, and any problems. The server resolves the files on every use, so editing a file on disk takes effect on the next op; the GUI refetches on its Reload button.

## Editing components

| Op | |
|---|---|
| `entity.create {origin, angles?, components?, layer?, group?}` | a point entity; `components` is `{name: data}`, stored as given |
| `entity.create_brush_entity {objects, origin?, angles?, components?}` | an entity that owns the objects; the origin defaults to the centre of their bounds |
| `entity.add_component {ids, name, data?}` | fails if present; `data` defaults to the schema's top-level property defaults |
| `entity.remove_component {ids, name}` | fails if absent |
| `entity.set_component {ids, name, data}` | replaces the whole data object; fails if absent |
| `entity.update_component {ids, name, set?, remove?}` | merges top-level keys; fails if absent |
| `entity.set_transform {id, origin?, angles?, members?}` | `members` is `carry` (default: the members move with the pivot) or `keep` |
| `entity.release_objects {ids}` | returns members to the world; the entity stays, as a point entity |

## Links

In a component schema, a node with `"editor": {"linkType": "entityID"}` marks a string as the id of another entity (`"e7"`):

```json
{ "type": "object", "properties": { "link": { "type": "string", "editor": { "linkType": "entityID" } } } }
```

The walker (`editor_cmd::links`) follows `properties`, `additionalProperties`, `prefixItems`, `items`, `allOf`, `anyOf`, `oneOf` and local `$ref`s (`#/$defs/...`), so arrays of links and links nested in arrays of objects work. For `anyOf` and `oneOf` it takes the union of the branches. `patternProperties`, `if`/`then` and `dependentSchemas` are not followed.

`entity.links {ids?}` lists every link as `{from, component, pointer, value, status, to}`. `pointer` is a JSON pointer into that component's data; `status` is `ok`, `missing` (a valid entity id with no such entity), `not_entity` (some other kind of object) or `malformed`.

- `object.duplicate` and `object.array` rewrite links between entities that are copied together to point at the copies; a link to anything not copied is left alone.
- `object.delete` warns when a surviving entity still links to something it deleted.
- The viewports and `view.screenshot` (`overlays: ["links"]`) draw an arrow from each linking entity to its target (a point entity's origin, or the centre of a brush entity's bounds). Links touching the selection are highlighted; links that do not resolve are not drawn.

## Validation

| Where | What |
|---|---|
| `entity.create`, `create_brush_entity`, `add_component`, `set_component`, `update_component` | Warnings (never errors) for an unknown component (only when a definitions file contributes components) and schema violations. `null` and a bad component name are `InvalidArgument`. |
| `check.validate` | Issues with a typed `code` and `severity`. |

Issue codes for components and definitions: `defs_load_failed` (error: unreadable path, bad JSON, unsupported version), `defs_bad_schema` (error), `duplicate_component` (warning), `unknown_component` (warning), `schema_violation` (error, one per violation, with the component and JSON pointer), `dangling_link` (warning; missing or malformed target), `link_not_entity` (warning), `self_link` (warning). Material texture paths are checked too (`texture_unreadable`).

## GUI

- The inspector shows an entity's origin and angles (for a brush entity, "Move pivot only" edits them without moving the brushes) and one collapsible section per component: its description, its data as pretty JSON in a text area, Apply, Revert and Remove. A syntax error or a `null` disables Apply; schema violations are listed with their pointers but do not block it. Apply sends `entity.set_component`. A `+ add` combo adds any catalog component the entity lacks. The links in and out of the entity are listed (click to select).
- The Entities tab (left panel) is the component browser: components by category with a colour swatch and search, plus the definitions records. Drag a component into a viewport to create a point entity with it: it lands on the surface under the cursor (perspective, offset 8 units along the surface normal), the ground, or, in an orthographic view, on the plane through the selection; positions snap to the grid. The `+` button adds the component to the selected entities; with none selected it makes an entity of the selected world brushes and meshes, and with neither it places a point entity at the middle of the selection or scene. "Entity from selection" makes a component-less entity of the selected geometry.
- View > Entity links toggles the arrows.

A form generated from the schema is a later step.
