//! Confirms the GUI's load/save plumbing uses the same serde format as the
//! CLI: load the sample character, save it back out, reload, and check the
//! struct is unchanged. If this drifts, the GUI will silently corrupt user
//! files.

use std::fs;
use std::path::PathBuf;

use exalted::ui::io::{load_character_from_path, save_character_to_path};

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sample-character.toml")
}

#[test]
fn load_save_load_round_trip_matches_original() {
    // Initialise the rules database — character validation isn't exercised
    // here, but parsing certain refs may transitively touch the db.
    let _ = exalted::rules::database::init_database();

    let original = load_character_from_path(&sample_path()).expect("load sample");

    let tmp_dir = std::env::temp_dir();
    let tmp = tmp_dir.join("exalted-ui-roundtrip.toml");

    save_character_to_path(&original, &tmp).expect("save");
    let reloaded = load_character_from_path(&tmp).expect("reload");

    assert_eq!(
        original, reloaded,
        "ui io path mutated the character on round-trip"
    );

    // Cleanup; ignore failures.
    let _ = fs::remove_file(&tmp);
}
