# Schema-first types

The committed JSON Schema files in `schemas/` are the source of truth for the shape of data that crosses a boundary. The Rust types are generated from them at build time and are never edited by hand. Domain methods live in `impl` blocks next to the generated type.

## Layout

`schemas/<name>.schema.json`, draft 2020-12. Every type is a named entry in `$defs`. A definition name is unique across all files: `editor_schema::bundle` merges every file into one namespace, rewrites cross-file `$ref`s (`other.schema.json#/$defs/X`) to local ones (`#/$defs/X`) and refuses duplicates. The files are listed in `crates/editor-schema/src/sources.rs`; add a new file there.

## Pipeline

1. A crate that owns generated types has a `build.rs` that calls `editor_schema_gen::generate(&Config { roots, replacements, output })`.
2. `generate` takes the bundled definitions, keeps those reachable from `roots` (minus `replacements`), runs `typify`, appends `impl editor_schema::Schema` for each root, and writes `$OUT_DIR/<output>.rs`.
3. The crate includes it (`include!(concat!(env!("OUT_DIR"), "/<output>.rs"))`) in a private module and re-exports the types.
4. A `replacement` is a definition that already exists as a hand-written Rust type, referenced instead of generated: `("Vec3", "glam::DVec3")`. Use it only for leaf types with behaviour of their own.

Editing a schema changes `editor-schema`, which rebuilds every dependent build script.

## Runtime

`editor-schema` has no editor dependencies.

- `parse::<T>(&Value)`, `parse_str`, `parse_slice`: validate against the schema of `T`, then deserialize. A `ParseError` is `Syntax`, `Invalid(Vec<Violation>)` (each with the instance JSON pointer, the schema pointer and a message) or `Deserialize` (the input satisfies the schema but not the type: the two disagree, which is a bug).
- `CompiledSchema::compile(&Value)` and `validate_dynamic` are for schemas that are not committed, namely an entity class's property schema.
- `schema_value(name)` returns the standalone schema of a definition, for `schema` commands and MCP.

All untrusted input goes through these. Do not `serde_json::from_*` a generated type directly.

## Writing schemas for typify (0.6)

- Close every object: `"additionalProperties": false` becomes `deny_unknown_fields`. A test fails an object with `properties` that does not say. Leave one open only when its shape truly varies, and explain in `description`.
- A `pattern` makes typify emit a newtype that uses the `regress` crate, so a crate that generates one needs `regress` as a dependency (`editor-core` and `editor-sim` have it).
- `const` is erased to `serde_json::Value`; use a one-value `enum`.
- A tagged union of records: put the tag as a one-value `enum` property in each record and use `oneOf` of `$ref`s. Typify produces an untagged enum of structs; the tag makes the choice unambiguous. `allOf` of a `$ref` plus a tag inside a `oneOf` generates empty enums.
- Struct fields are generated in alphabetical order, so serialized output is key-sorted. Anything that must be byte-stable (the embedded defs JSON in a scene) is sorted.
- `format: "float"` gives `f32`; plain `number` is `f64`. Integer bounds pick the Rust integer type.
- A property with no `type` is a `serde_json::Value`. Give `default` for a field that may be omitted.
- Name union variants with `title`, or typify numbers them.

## Migration status

- Done: the component definitions file (`component-defs.schema.json`: `ComponentDefsFile`, `ComponentDef`, `ComponentSize`) and the built-in component data types (`components.schema.json`: `KeyframeAnimation`, `Keyframe`, `Interpolation`, `PlayerStart`, generated in `editor-sim`; `editor-cmd` also reads the same definitions as the schemas of the built-in components). The `schemars` types that embed them (`DefsSource`, op args and results) reach them through the temporary bridge in `editor-core/src/legacy_schemars.rs`.
- Not yet: the remaining model records, op args and results, the envelope, the wire protocol and the scene format. Those still derive `schemars::JsonSchema`.
