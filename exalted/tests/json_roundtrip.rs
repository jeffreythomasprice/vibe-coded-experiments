mod common;

use common::valid_dawn;

#[test]
fn full_character_round_trips_through_json() {
    let original = valid_dawn();
    let json = serde_json::to_string_pretty(&original).expect("serialize");
    let decoded: exalted::Character = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(original, decoded, "round-trip changed the character");
}

#[test]
fn blank_solar_round_trips() {
    let c = exalted::Character::new_blank_solar(
        "Sketch",
        exalted::character::Caste::Twilight,
    );
    let json = serde_json::to_string(&c).expect("serialize");
    let decoded: exalted::Character = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(c, decoded);
}
