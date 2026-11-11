use std::collections::BTreeSet;

use editor_core::ObjectId;

/// The object to select on a drill click: the hit behind the nearest already-selected one, wrapping,
/// or the nearest hit when none of them is selected. `hits` is ordered nearest first.
pub fn next_hit(hits: &[ObjectId], selection: &BTreeSet<ObjectId>) -> Option<ObjectId> {
    let selected = hits.iter().position(|id| selection.contains(id));
    match selected {
        Some(i) => hits.get((i + 1) % hits.len()).copied(),
        None => hits.first().copied(),
    }
}

#[cfg(test)]
mod tests {
    use editor_core::BrushId;

    use super::*;

    fn brush(n: u64) -> ObjectId {
        BrushId(n).into()
    }

    #[test]
    fn nothing_selected_picks_the_nearest() {
        let hits = [brush(3), brush(1), brush(2)];
        assert_eq!(next_hit(&hits, &BTreeSet::new()), Some(brush(3)));
    }

    #[test]
    fn a_selected_hit_moves_on_to_the_one_behind_it() {
        let hits = [brush(3), brush(1), brush(2)];
        let selection = BTreeSet::from([brush(3)]);
        assert_eq!(next_hit(&hits, &selection), Some(brush(1)));
    }

    #[test]
    fn the_farthest_wraps_to_the_nearest() {
        let hits = [brush(3), brush(1), brush(2)];
        let selection = BTreeSet::from([brush(2)]);
        assert_eq!(next_hit(&hits, &selection), Some(brush(3)));
    }

    #[test]
    fn a_selection_off_the_ray_starts_from_the_nearest() {
        let hits = [brush(3), brush(1)];
        let selection = BTreeSet::from([brush(9)]);
        assert_eq!(next_hit(&hits, &selection), Some(brush(3)));
    }

    #[test]
    fn no_hits_selects_nothing() {
        assert_eq!(next_hit(&[], &BTreeSet::from([brush(1)])), None);
    }
}
