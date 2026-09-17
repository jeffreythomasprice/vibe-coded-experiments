//! Generates a placeholder player name ("Brave Otter") when someone hasn't picked one -- so an
//! invite-link join never lands in a room under a blank name, and the multiplayer form is never
//! blank either. Uses `petname`'s public English word lists directly (`Petnames::small()`'s
//! `adjectives`/`nouns` fields) rather than its own `namer`/`petname` API, which is RNG-backed and
//! would pull in `getrandom` -- see the comment on the `petname` dependency in `Cargo.toml`.

use petname::Petnames;

/// Picks one word by a roll in `[0, 1)`. Separated from `random_player_name` so the only part with
/// logic in it is testable natively -- this crate has no wasm-bindgen-test setup, so anything that
/// actually calls into `js_sys` can't be exercised by `cargo test --workspace`.
fn pick<'a>(words: &[&'a str], roll: f64) -> Option<&'a str> {
    if words.is_empty() {
        return None;
    }
    // `roll` is expected to be in [0, 1), but never trust a caller (or a stray `NaN`) not to
    // index past the end -- the final `.min` catches both an out-of-range roll and the case
    // `roll == 1.0` landing exactly on `words.len()`.
    let roll = if roll.is_finite() { roll.clamp(0.0, 1.0) } else { 0.0 };
    let index = ((roll * words.len() as f64) as usize).min(words.len() - 1);
    Some(words[index])
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// "Brave Otter" -- capitalized and space-separated so it reads like a name a person would type,
/// not a machine-generated slug.
fn name_from(adjectives: &[&str], nouns: &[&str], adjective_roll: f64, noun_roll: f64) -> String {
    let adjective = pick(adjectives, adjective_roll).map(capitalize);
    let noun = pick(nouns, noun_roll).map(capitalize);
    match (adjective, noun) {
        (Some(adjective), Some(noun)) => format!("{adjective} {noun}"),
        (Some(word), None) | (None, Some(word)) => word,
        (None, None) => String::new(),
    }
}

/// "Brave Otter" -- drawn from `petname`'s small English word lists (449 adjectives × 449 nouns).
/// Shared by `random_player_name` and `random_room_name`: both need the same shape of name and the
/// same 40-character bound, just for different fields.
fn random_two_word_name() -> String {
    let words = Petnames::small();
    name_from(&words.adjectives, &words.nouns, js_sys::Math::random(), js_sys::Math::random())
}

/// A random display name. Every combination comfortably fits `shared::protocol::MAX_NAME_LEN` --
/// see the `every_generated_name_fits_the_protocols_bound` test.
pub fn random_player_name() -> String {
    random_two_word_name()
}

/// A random room name, suggested when opening the host form and on each click of its randomize
/// button. Same word lists as `random_player_name` -- `MAX_ROOM_NAME_LEN` and `MAX_NAME_LEN` are
/// both 40, so every combination that fits one fits the other too; see
/// `every_generated_room_name_survives_room_key`.
pub fn random_room_name() -> String {
    random_two_word_name()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::protocol::{sanitize_name, MAX_NAME_LEN};

    #[test]
    fn pick_maps_zero_to_the_first_word() {
        assert_eq!(pick(&["a", "b", "c"], 0.0), Some("a"));
    }

    #[test]
    fn pick_maps_just_under_one_to_the_last_word() {
        assert_eq!(pick(&["a", "b", "c"], 0.999_999), Some("c"));
    }

    #[test]
    fn pick_returns_none_for_an_empty_list() {
        assert_eq!(pick(&[], 0.5), None);
    }

    #[test]
    fn pick_clamps_rather_than_indexing_past_the_end() {
        assert_eq!(pick(&["a", "b", "c"], 1.0), Some("c"));
        assert_eq!(pick(&["a", "b", "c"], 2.0), Some("c"));
        assert_eq!(pick(&["a", "b", "c"], f64::NAN), Some("a"));
        assert_eq!(pick(&["a", "b", "c"], -1.0), Some("a"));
    }

    #[test]
    fn name_from_capitalizes_and_joins_with_one_space() {
        assert_eq!(name_from(&["brave"], &["otter"], 0.0, 0.0), "Brave Otter");
    }

    #[test]
    fn name_from_with_one_empty_list_yields_the_other_word_alone() {
        assert_eq!(name_from(&[], &["otter"], 0.0, 0.0), "Otter");
        assert_eq!(name_from(&["brave"], &[], 0.0, 0.0), "Brave");
    }

    #[test]
    fn every_generated_name_fits_the_protocols_bound() {
        let words = Petnames::small();
        for adjective in words.adjectives.iter() {
            for noun in words.nouns.iter() {
                let name = format!("{} {}", capitalize(adjective), capitalize(noun));
                assert!(name.chars().count() <= MAX_NAME_LEN, "{name:?} is too long");
                assert_eq!(sanitize_name(&name).to_string(), name);
            }
        }
    }

    #[test]
    fn every_generated_room_name_survives_room_key() {
        use shared::protocol::room_key;

        let words = Petnames::small();
        for adjective in words.adjectives.iter() {
            for noun in words.nouns.iter() {
                let name = format!("{} {}", capitalize(adjective), capitalize(noun));
                assert!(room_key(&name).is_ok(), "{name:?} was rejected as a room name");
            }
        }
    }
}
