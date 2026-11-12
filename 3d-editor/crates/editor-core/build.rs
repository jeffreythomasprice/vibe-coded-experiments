use editor_schema_gen::{Config, generate};

fn main() {
    let result = generate(&Config {
        roots: &["ComponentDefsFile", "ComponentDef", "ComponentSize"],
        replacements: &[],
        output: "component_defs",
    });
    if let Err(e) = result {
        eprintln!("editor-core: {e}");
        std::process::exit(1);
    }
}
