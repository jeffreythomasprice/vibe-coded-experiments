//! The quicksave rotation: which slot a quicksave overwrites and which one a
//! quickload restores.
//!
//! Only [`StateSlot::Quick`] states take part. Those numbered past `count` (say,
//! after the setting was lowered) are left out of the rotation.

use crate::storage::{StateEntry, StateSlot};

/// The 1-based slot number the next quicksave should write: the lowest unused
/// one, or once all `count` exist, the least recently modified. A `count` of zero
/// is treated as one.
pub fn next_quicksave(states: &[StateEntry], count: u32) -> u32 {
    let count = count.max(1);
    let existing = quicksaves(states, count);
    // Among n existing quicksaves, a gap must show up within the first n + 1.
    let unused = (1..=count).find(|number| existing.iter().all(|(used, _)| used != number));
    unused.unwrap_or_else(|| {
        existing
            .iter()
            .min_by_key(|(number, state)| (state.modified, *number))
            .map_or(1, |(number, _)| *number)
    })
}

/// The most recently modified quicksave, if there is one.
pub fn latest_quicksave(states: &[StateEntry], count: u32) -> Option<&StateEntry> {
    quicksaves(states, count.max(1))
        .into_iter()
        .max_by_key(|(number, state)| (state.modified, *number))
        .map(|(_, state)| state)
}

/// The states that belong to the rotation, with their numbers.
fn quicksaves(states: &[StateEntry], count: u32) -> Vec<(u32, &StateEntry)> {
    states
        .iter()
        .filter_map(|state| match state.slot {
            StateSlot::Quick(number) if (1..=count).contains(&number) => Some((number, state)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn entry(slot: StateSlot, secs: u64) -> StateEntry {
        StateEntry {
            slot,
            modified: UNIX_EPOCH + Duration::from_secs(secs),
        }
    }

    /// Quicksaves given as `(number, seconds)`.
    fn states(entries: &[(u32, u64)]) -> Vec<StateEntry> {
        entries
            .iter()
            .map(|&(number, secs)| entry(StateSlot::Quick(number), secs))
            .collect()
    }

    #[test]
    fn fills_unused_slots_in_order() {
        assert_eq!(next_quicksave(&[], 5), 1);
        assert_eq!(next_quicksave(&states(&[(1, 1), (3, 2)]), 5), 2);
    }

    #[test]
    fn a_full_rotation_overwrites_the_oldest() {
        assert_eq!(next_quicksave(&states(&[(1, 30), (2, 10), (3, 20)]), 3), 2);
    }

    #[test]
    fn repeated_saves_cycle_through_every_slot() {
        let mut existing: Vec<StateEntry> = Vec::new();
        let mut written = Vec::new();
        for secs in 0..7 {
            let number = next_quicksave(&existing, 3);
            let slot = StateSlot::Quick(number);
            existing.retain(|state| state.slot != slot);
            existing.push(entry(slot.clone(), secs));
            assert_eq!(latest_quicksave(&existing, 3).unwrap().slot, slot);
            written.push(number);
        }
        assert_eq!(written, [1, 2, 3, 1, 2, 3, 1]);
    }

    #[test]
    fn latest_is_by_date_and_ignores_other_saves() {
        let mut existing = states(&[(1, 5), (2, 9)]);
        existing.push(entry(StateSlot::Manual("quicksave-3".to_string()), 100));
        existing.push(entry(StateSlot::Auto, 200));
        let latest = latest_quicksave(&existing, 5).unwrap();
        assert_eq!(latest.slot, StateSlot::Quick(2));
        assert_eq!(next_quicksave(&existing, 5), 3);
    }

    #[test]
    fn numbers_outside_the_rotation_are_ignored() {
        let outside = states(&[(0, 1), (3, 2)]);
        assert_eq!(latest_quicksave(&outside, 2), None);
        assert_eq!(next_quicksave(&outside, 2), 1);
    }

    #[test]
    fn a_zero_count_still_keeps_one_quicksave() {
        let one = states(&[(1, 1)]);
        assert_eq!(next_quicksave(&one, 0), 1);
        assert!(latest_quicksave(&one, 0).is_some());
    }
}
