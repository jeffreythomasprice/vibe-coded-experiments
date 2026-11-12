use editor_schema_gen::{Config, generate};

fn main() {
    let result = generate(&Config {
        roots: &["KeyframeAnimation", "PlayerStart"],
        replacements: &[],
        output: "components",
    });
    if let Err(e) = result {
        eprintln!("editor-sim: {e}");
        std::process::exit(1);
    }
}
