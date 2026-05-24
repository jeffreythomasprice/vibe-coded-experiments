mod common;

use common::valid_dawn;

#[test]
fn full_character_round_trips_through_toml() {
    let original = valid_dawn();
    let text = toml::to_string_pretty(&original).expect("serialize");
    let decoded: exalted::Character = toml::from_str(&text).expect("deserialize");
    assert_eq!(original, decoded, "round-trip changed the character");
}

#[test]
fn blank_solar_round_trips() {
    let c = exalted::Character::new_blank_solar("Sketch", exalted::character::Caste::Twilight);
    let text = toml::to_string(&c).expect("serialize");
    let decoded: exalted::Character = toml::from_str(&text).expect("deserialize");
    assert_eq!(c, decoded);
}
