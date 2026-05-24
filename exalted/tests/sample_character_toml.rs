mod common;

use std::path::PathBuf;

use common::valid_dawn_with_notes_demo;

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sample-character.toml")
}

#[test]
fn sample_matches_valid_dawn_fixture() {
    let expected = valid_dawn_with_notes_demo();
    let text = std::fs::read_to_string(sample_path()).expect(
        "assets/sample-character.toml missing; run \
         `cargo test --test sample_character_toml -- --ignored regen_sample_character_toml` to regenerate",
    );
    let actual: exalted::Character =
        toml::from_str(&text).expect("parse sample-character.toml");
    assert_eq!(
        actual, expected,
        "assets/sample-character.toml does not match the valid_dawn_with_notes_demo() fixture; \
         regenerate with `cargo test --test sample_character_toml -- --ignored regen_sample_character_toml`"
    );
}

#[test]
#[ignore]
fn regen_sample_character_toml() {
    let c = valid_dawn_with_notes_demo();
    let toml_text = toml::to_string_pretty(&c).expect("serialize");
    std::fs::write(sample_path(), toml_text).expect("write asset");
}
