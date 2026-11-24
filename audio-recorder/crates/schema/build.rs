use std::{env, fmt::Write as _, fs, path::PathBuf};

use typify::{TypeSpace, TypeSpaceSettings};

enum Targets {
    Root(&'static str),
    AllDefinitions,
}

const SCHEMAS: &[(&str, &str, Targets)] = &[
    ("config", "config.schema.json", Targets::Root("Config")),
    ("device_state", "device-state.schema.json", Targets::Root("DeviceStateFile")),
    ("manifest", "manifest.schema.json", Targets::Root("Manifest")),
    ("ipc", "ipc.schema.json", Targets::AllDefinitions),
    ("theme", "theme.schema.json", Targets::Root("Theme")),
    ("settings", "settings.schema.json", Targets::Root("Settings")),
    ("mix", "mix.schema.json", Targets::Root("MixSettings")),
    ("analysis", "analysis.schema.json", Targets::Root("Analysis")),
    ("transcript", "transcript.schema.json", Targets::Root("Transcript")),
    ("jobs", "jobs.schema.json", Targets::Root("Job")),
    ("summary", "summary.schema.json", Targets::Root("Summary")),
    ("llm", "llm.schema.json", Targets::AllDefinitions),
];

fn main() {
    let schema_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../schemas");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed={}", schema_dir.display());

    let mut generated = String::new();
    let mut registry = String::from("fn register_all(map: &mut HashMap<&'static str, jsonschema::Validator>) {\n");

    for (module, file, targets) in SCHEMAS {
        let path = schema_dir.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let raw: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let schema: schemars::schema::RootSchema =
            serde_json::from_value(raw.clone()).unwrap_or_else(|e| panic!("schema {}: {e}", path.display()));

        let mut space = TypeSpace::new(&TypeSpaceSettings::default());
        space
            .add_root_schema(schema)
            .unwrap_or_else(|e| panic!("typify {}: {e}", path.display()));
        let tokens = space
            .to_stream()
            .unwrap_or_else(|e| panic!("codegen {}: {e}", path.display()));

        let mut code = tokens.to_string();
        let targets: Vec<(String, PathBuf, String)> = match targets {
            Targets::Root(root) => vec![(root.to_string(), path.clone(), module.to_string())],
            Targets::AllDefinitions => {
                let defs = raw["definitions"].as_object().expect("definitions object");
                defs.keys()
                    .map(|name| {
                        let mut wrapper = serde_json::Map::new();
                        wrapper.insert("$schema".into(), raw["$schema"].clone());
                        wrapper.insert("$ref".into(), format!("#/definitions/{name}").into());
                        wrapper.insert("definitions".into(), raw["definitions"].clone());
                        let wrapper_path = out_dir.join(format!("{module}.{name}.schema.json"));
                        fs::write(&wrapper_path, serde_json::to_string(&wrapper).unwrap()).unwrap();
                        (name.clone(), wrapper_path, format!("{module}.{name}"))
                    })
                    .collect()
            }
        };
        for (type_name, json_path, schema_name) in &targets {
            let _ = write!(
                code,
                "\nimpl crate::Schema for {type_name} {{ const NAME: &'static str = \"{schema_name}\"; const SCHEMA_JSON: &'static str = include_str!(\"{}\"); }}\n",
                json_path.display()
            );
            let _ = writeln!(registry, "    register::<{module}::{type_name}>(map);");
        }

        let formatted = match syn::parse_file(&code) {
            Ok(file) => prettyplease::unparse(&file),
            Err(_) => code,
        };
        let module_file = out_dir.join(format!("{module}.rs"));
        fs::write(&module_file, formatted).unwrap();
        let _ = writeln!(
            generated,
            "pub mod {module} {{\n    #![allow(clippy::all, unused_imports, dead_code)]\n    include!(\"{}\");\n}}",
            module_file.display()
        );
    }
    registry.push_str("}\n");
    generated.push_str(&registry);
    fs::write(out_dir.join("generated.rs"), generated).unwrap();
}
