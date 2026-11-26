//! The quicksave rotation: which save state a quicksave overwrites and which one
//! a quickload restores.
//!
//! Quicksaves are ordinary save states recognised purely by name
//! (`quicksave-1` ..= `quicksave-{count}`), so a state a user creates under one
//! of those names takes part in the rotation like any other. Names numbered
//! past `count` are just normal saves.

use crate::storage::StateEntry;

const PREFIX: &str = "quicksave-";

/// The state name of the 1-based quicksave `index`.
pub fn quicksave_name(index: u32) -> String {
    format!("{PREFIX}{index}")
}

/// The name the next quicksave should write: the lowest unused one, or once all
/// `count` exist, the least recently modified. A `count` of zero is treated as
/// one.
pub fn next_quicksave(states: &[StateEntry], count: u32) -> String {
    let count = count.max(1);
    let existing = quicksaves(states, count);
    // Among n existing quicksaves, a gap must show up within the first n + 1.
    let unused = (1..=count).find(|index| existing.iter().all(|(used, _)| used != index));
    let index = unused.unwrap_or_else(|| {
        existing
            .iter()
            .min_by_key(|(index, state)| (state.modified, *index))
            .map_or(1, |(index, _)| *index)
    });
    quicksave_name(index)
}

/// The most recently modified quicksave, if there is one.
pub fn latest_quicksave(states: &[StateEntry], count: u32) -> Option<&StateEntry> {
    quicksaves(states, count.max(1))
        .into_iter()
        .max_by_key(|(index, state)| (state.modified, *index))
        .map(|(_, state)| state)
}

/// The states that belong to the rotation, with their indices.
fn quicksaves(states: &[StateEntry], count: u32) -> Vec<(u32, &StateEntry)> {
    states
        .iter()
        .filter_map(|state| {
            let index: u32 = state.name.strip_prefix(PREFIX)?.parse().ok()?;
            // Only the canonical spelling counts, not e.g. `quicksave-01`.
            let canonical = state.name == quicksave_name(index);
            (canonical && (1..=count).contains(&index)).then_some((index, state))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn states(entries: &[(&str, u64)]) -> Vec<StateEntry> {
        entries
            .iter()
            .map(|&(name, secs)| StateEntry {
                name: name.to_string(),
                modified: UNIX_EPOCH + Duration::from_secs(secs),
            })
            .collect()
    }

    fn latest(entries: &[(&str, u64)], count: u32) -> Option<String> {
        latest_quicksave(&states(entries), count).map(|s| s.name.clone())
    }

    #[test]
    fn fills_unused_names_in_order() {
        assert_eq!(next_quicksave(&[], 5), "quicksave-1");
        let some = states(&[("quicksave-1", 1), ("quicksave-3", 2), ("boss", 3)]);
        assert_eq!(next_quicksave(&some, 5), "quicksave-2");
    }

    #[test]
    fn a_full_rotation_overwrites_the_oldest() {
        let full = states(&[("quicksave-1", 30), ("quicksave-2", 10), ("quicksave-3", 20)]);
        assert_eq!(next_quicksave(&full, 3), "quicksave-2");
    }

    #[test]
    fn repeated_saves_cycle_through_every_slot() {
        let mut existing: Vec<StateEntry> = Vec::new();
        let mut written = Vec::new();
        for secs in 0..7 {
            let name = next_quicksave(&existing, 3);
            existing.retain(|state| state.name != name);
            existing.extend(states(&[(name.as_str(), secs)]));
            assert_eq!(latest_quicksave(&existing, 3).unwrap().name, name);
            written.push(name);
        }
        let expected: Vec<String> = [1, 2, 3, 1, 2, 3, 1].map(quicksave_name).into();
        assert_eq!(written, expected);
    }

    #[test]
    fn latest_is_by_date_and_ignores_other_saves() {
        let entries = [("quicksave-1", 5), ("quicksave-2", 9), ("manual", 100)];
        assert_eq!(latest(&entries, 5).as_deref(), Some("quicksave-2"));
        assert_eq!(latest(&[("manual", 1)], 5), None);
    }

    #[test]
    fn names_outside_the_rotation_are_ordinary_saves() {
        let entries = [
            ("quicksave-0", 1),
            ("quicksave-01", 2),
            ("quicksave-3", 3),
            ("quicksave-x", 4),
            ("Quicksave-1", 5),
        ];
        assert_eq!(latest(&entries, 2), None);
        assert_eq!(next_quicksave(&states(&entries), 2), "quicksave-1");
    }

    #[test]
    fn a_zero_count_still_keeps_one_quicksave() {
        let one = states(&[("quicksave-1", 1)]);
        assert_eq!(next_quicksave(&one, 0), "quicksave-1");
        assert!(latest_quicksave(&one, 0).is_some());
    }
}
