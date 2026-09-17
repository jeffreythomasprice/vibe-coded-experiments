//! Compiles `shared/schemas/*.json` into `$OUT_DIR/wire.rs` via typify. See
//! `shared/schemas/README.md` for the schema authoring rules this depends on; see
//! `shared/src/generated.rs` for where the output lands.

use schemars::schema::Schema;
use std::collections::BTreeMap;
use std::path::Path;
use typify::{TypeSpace, TypeSpaceImpl, TypeSpacePatch, TypeSpaceSettings};

const SCHEMA_DIR: &str = "schemas";

/// Types this crate hand-writes rather than generates (see `shared/schemas/README.md`), and what
/// typify should substitute wherever a schema `$ref`s them. `Index` isn't hand-written so much as
/// unrepresentable: JSON Schema's "uint" format is the only one typify maps to a Rust type without
/// a fixed width, and it always picks `u32` for that, so a def that needs `usize` has to be routed
/// through this same mechanism.
const REPLACEMENTS: &[(&str, &str)] = &[
    ("BattleLog", "crate::battle::BattleLog"),
    ("Timestamp", "crate::timestamp::Timestamp"),
    ("SessionRejection", "crate::protocol::SessionRejection"),
    ("Index", "usize"),
];

/// `(def name, extra derives)` for the generated types that need more than typify's default
/// `Debug, Clone, Serialize, Deserialize` plus this build's blanket `PartialEq, Eq` (see below) --
/// matched one-for-one against what the equivalent hand-written type used to derive, so downstream
/// code needs no changes for a missing trait.
const DERIVE_PATCHES: &[(&str, &[&str])] = &[
    ("ConnectionId", &["Hash"]),
    ("RequestId", &["Copy", "Hash"]),
    ("CombatantId", &["Copy", "Hash", "PartialOrd", "Ord"]),
    ("MarkerId", &["Copy", "Hash", "PartialOrd", "Ord"]),
    ("Side", &["Hash"]),
    ("ActionKind", &["Copy", "Hash"]),
    ("BattleMode", &["Copy", "Hash"]),
    ("JoinBattleResult", &["Copy"]),
    ("SpeedSpec", &["Copy"]),
    ("DvPenaltySpec", &["Copy"]),
    ("DvState", &["Copy", "Default"]),
    ("CreateAccessCode", &["Default"]),
];

/// The merged `$defs`, in both the `schemars::Schema` form typify consumes and the raw
/// `serde_json::Value` form written out as `wire.schema.json` for runtime validation (see
/// `shared/src/validate.rs`) -- built in one pass so the two can never see a different set of
/// files.
fn merged_defs(schema_dir: &Path) -> (BTreeMap<String, Schema>, serde_json::Map<String, serde_json::Value>) {
    let mut typed = BTreeMap::new();
    let mut raw = serde_json::Map::new();
    let mut entries: Vec<_> =
        std::fs::read_dir(schema_dir).expect("read shared/schemas").filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        println!("cargo::rerun-if-changed={}", path.display());

        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        let defs = document
            .get("$defs")
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("{}: expected a top-level \"$defs\" object", path.display()));

        for (name, schema) in defs {
            let typed_schema: Schema = serde_json::from_value(schema.clone())
                .unwrap_or_else(|error| panic!("{}: def {name}: {error}", path.display()));
            if typed.insert(name.clone(), typed_schema).is_some() || raw.insert(name.clone(), schema.clone()).is_some()
            {
                panic!("def {name} is defined in more than one schema file (last seen in {})", path.display());
            }
        }
    }

    (typed, raw)
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let schema_dir = Path::new(&manifest_dir).join(SCHEMA_DIR);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");

    let (typed_defs, raw_defs) = merged_defs(&schema_dir);

    let schema_doc = serde_json::json!({ "$defs": raw_defs });
    let schema_path = Path::new(&out_dir).join("wire.schema.json");
    std::fs::write(&schema_path, serde_json::to_string(&schema_doc).expect("merged schema serializes"))
        .unwrap_or_else(|error| panic!("write {}: {error}", schema_path.display()));

    let mut settings = TypeSpaceSettings::default();
    settings.with_derive("PartialEq".to_string());
    settings.with_derive("Eq".to_string());

    for (name, replace_type) in REPLACEMENTS {
        settings.with_replacement(*name, *replace_type, std::iter::empty::<TypeSpaceImpl>());
    }

    let mut patches = Vec::new();
    for (name, derives) in DERIVE_PATCHES {
        let mut patch = TypeSpacePatch::default();
        for derive in *derives {
            patch.with_derive(*derive);
        }
        patches.push((*name, patch));
    }
    for (name, patch) in &patches {
        settings.with_patch(*name, patch);
    }

    let mut type_space = TypeSpace::new(&settings);
    type_space.add_ref_types(typed_defs).expect("generate Rust types from shared/schemas");

    let tokens = type_space.to_stream();
    let file: syn::File = syn::parse2(tokens).expect("generated code must parse as a Rust file");
    let formatted = prettyplease::unparse(&file);

    let out_path = Path::new(&out_dir).join("wire.rs");
    std::fs::write(&out_path, formatted).unwrap_or_else(|error| panic!("write {}: {error}", out_path.display()));
}
