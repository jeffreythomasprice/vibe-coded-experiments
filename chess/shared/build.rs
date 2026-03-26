use std::{env, fs, path::Path};
use typify::{TypeSpace, TypeSpaceSettings};

fn main() {
    let schema_dir = Path::new("schemas");
    println!("cargo::rerun-if-changed={}", schema_dir.display());

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("codegen.rs");

    let mut all_code = String::new();

    for entry in glob::glob("schemas/*.json").expect("failed to glob schemas") {
        let path = entry.expect("glob error");
        println!("cargo::rerun-if-changed={}", path.display());

        let content = fs::read_to_string(&path).unwrap();
        let schema =
            serde_json::from_str::<schemars::schema::RootSchema>(&content).unwrap();

        let mut type_space = TypeSpace::new(&TypeSpaceSettings::default());
        type_space.add_root_schema(schema).unwrap();

        let tokens = type_space.to_stream();
        let parsed = syn::parse2::<syn::File>(tokens).unwrap();
        let formatted = prettyplease::unparse(&parsed);

        all_code.push_str(&formatted);
        all_code.push('\n');
    }

    fs::write(&out_path, all_code).unwrap();
}
