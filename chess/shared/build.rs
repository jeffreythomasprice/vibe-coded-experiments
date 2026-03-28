use std::{collections::BTreeMap, env, fs, path::Path};
use typify::{TypeSpace, TypeSpaceSettings};

fn main() {
    let schema_dir = Path::new("schemas");
    println!("cargo::rerun-if-changed={}", schema_dir.display());

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("codegen.rs");

    // Load all schemas and collect every type (root schemas + definitions) into one map.
    let mut all_types: BTreeMap<String, schemars::schema::Schema> = BTreeMap::new();

    for entry in glob::glob("schemas/*.json").expect("failed to glob schemas") {
        let path = entry.expect("glob error");
        println!("cargo::rerun-if-changed={}", path.display());

        let content = fs::read_to_string(&path).unwrap();
        let schema =
            serde_json::from_str::<schemars::schema::RootSchema>(&content).unwrap();

        // Collect explicit definitions.
        for (name, def) in &schema.definitions {
            all_types.insert(name.clone(), def.clone());
        }

        // Register root schema by its title so other schemas can $ref it.
        if let Some(title) = schema
            .schema
            .metadata
            .as_ref()
            .and_then(|m| m.title.as_ref())
        {
            all_types.insert(
                title.clone(),
                schemars::schema::Schema::Object(schema.schema.clone()),
            );
        }
    }

    // Generate types in a single TypeSpace using add_ref_types only.
    // This avoids duplicates from calling add_root_schema per file.
    let mut type_space = TypeSpace::new(&TypeSpaceSettings::default());
    type_space.add_ref_types(all_types).unwrap();

    let tokens = type_space.to_stream();
    let parsed = syn::parse2::<syn::File>(tokens).unwrap();
    let formatted = prettyplease::unparse(&parsed);

    fs::write(&out_path, formatted).unwrap();
}
