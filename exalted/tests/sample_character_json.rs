mod common;

use std::path::PathBuf;

use common::valid_dawn;

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sample-character.json")
}

#[test]
fn sample_matches_valid_dawn_fixture() {
    let expected = valid_dawn();
    let bytes = std::fs::read(sample_path()).expect(
        "assets/sample-character.json missing; run \
         `cargo test --test sample_character_json -- --ignored regen_sample_character_json` to regenerate",
    );
    let actual: exalted::Character =
        serde_json::from_slice(&bytes).expect("parse sample-character.json");
    assert_eq!(
        actual, expected,
        "assets/sample-character.json does not match the valid_dawn() fixture; \
         regenerate with `cargo test --test sample_character_json -- --ignored regen_sample_character_json`"
    );
}

#[test]
#[ignore]
fn regen_sample_character_json() {
    let c = valid_dawn();
    let json = serde_json::to_string_pretty(&c).expect("serialize");
    std::fs::write(sample_path(), json).expect("write asset");
}
