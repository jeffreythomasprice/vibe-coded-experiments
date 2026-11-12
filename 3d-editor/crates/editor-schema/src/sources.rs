macro_rules! schema_files {
    ($($name:literal),* $(,)?) => {
        pub const FILES: &[(&str, &str)] = &[
            $(($name, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/", $name)))),*
        ];
    };
}

schema_files!("component-defs.schema.json", "components.schema.json");
