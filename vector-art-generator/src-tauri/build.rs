use std::fs;
use std::path::Path;

fn main() {
    // Tauri build
    tauri_build::build();

    // Generate Rust types from JSON schemas using typify
    let schemas_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../schemas");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("generated_types.rs");

    let mut generated = String::new();
    generated.push_str("// Generated from schemas/ — do not edit manually\n\n");
    generated.push_str("#[allow(unused_imports)]\n");
    generated.push_str("use serde::{Deserialize, Serialize};\n\n");

    let mut type_space = typify::TypeSpace::default();

    let mut entries: Vec<_> = fs::read_dir(&schemas_dir)
        .expect("Failed to read schemas directory")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let content = fs::read_to_string(entry.path()).expect("Failed to read schema file");
        let schema: schemars::schema::RootSchema =
            serde_json::from_str(&content).expect("Failed to parse schema");
        type_space
            .add_root_schema(schema)
            .expect("Failed to add schema to type space");
    }

    let tokens = type_space.to_stream();
    generated.push_str(&tokens.to_string());

    fs::write(&out_path, generated).expect("Failed to write generated types");

    // Tell cargo to rerun if schemas change
    println!("cargo:rerun-if-changed=../schemas");
}
