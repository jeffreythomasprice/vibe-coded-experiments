use crate::battle::ids::{CombatantId, MarkerId, Tick};
use crate::battle::state::Battle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueItem {
    Combatant(CombatantId),
    Marker(MarkerId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueRow {
    pub at_tick: Tick,
    pub item: QueueItem,
}

/// Everything in flight, ordered as the queue panel shows it: combatants due now first, then by
/// tick ascending, combatants before markers within a tick, then by id for stability. Every
/// combatant appears (even ones far Beyond the Horizon on the wheel); markers appear whether
/// active or not yet started, but not once expired.
pub fn queue(battle: &Battle) -> Vec<QueueRow> {
    let now = battle.current_tick;
    let mut rows: Vec<QueueRow> = battle
        .combatants
        .iter()
        .map(|combatant| QueueRow { at_tick: combatant.next_action_tick, item: QueueItem::Combatant(combatant.id) })
        .chain(
            battle
                .active_markers()
                .chain(battle.pending_markers())
                .map(|marker| QueueRow { at_tick: marker.at_tick, item: QueueItem::Marker(marker.id) }),
        )
        .collect();

    rows.sort_by_key(|row| {
        let due_now = row.at_tick <= now && matches!(row.item, QueueItem::Combatant(_));
        let kind_rank = match row.item {
            QueueItem::Combatant(_) => 0,
            QueueItem::Marker(_) => 1,
        };
        let id_rank = match row.item {
            QueueItem::Combatant(id) => id.0,
            QueueItem::Marker(id) => id.0,
        };
        (!due_now, row.at_tick, kind_rank, id_rank)
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::action::{template, ActionKind, Declaration};
    use crate::battle::combatant::{JoinBattleResult, Side};
    use crate::battle::event::BattleEvent;
    use crate::battle::state::apply;

    fn add(battle: &mut Battle, id: u32, successes: u32) -> CombatantId {
        let cid = CombatantId(id);
        apply(
            battle,
            &BattleEvent::AddCombatant { id: cid, name: format!("C{id}"), side: Side("A".to_string()), join_battle: JoinBattleResult::Successes(successes) },
        )
        .unwrap();
        cid
    }

    #[test]
    fn ready_now_rows_sort_before_future_ones() {
        let mut battle = Battle::genesis();
        let fast = add(&mut battle, 1, 5);
        let slow = add(&mut battle, 2, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let rows = queue(&battle);
        assert_eq!(rows[0].item, QueueItem::Combatant(fast));
        assert_eq!(rows[1].item, QueueItem::Combatant(slow));
    }

    #[test]
    fn rows_order_by_tick_ascending() {
        let mut battle = Battle::genesis();
        let slow = add(&mut battle, 1, 0);
        let mid = add(&mut battle, 2, 3);
        let fast = add(&mut battle, 3, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let rows = queue(&battle);
        let items: Vec<QueueItem> = rows.iter().map(|r| r.item).collect();
        assert_eq!(items, vec![QueueItem::Combatant(fast), QueueItem::Combatant(mid), QueueItem::Combatant(slow)]);
    }

    #[test]
    fn combatants_sort_before_markers_on_the_same_tick() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        let dash = template(ActionKind::Dash).declare(Declaration::default());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: dash }).unwrap();
        apply(&mut battle, &BattleEvent::AddMarker { id: MarkerId(0), label: "Ambush".to_string(), source: cid, at_tick: 3, ticks: 1 }).unwrap();

        let rows = queue(&battle);
        let at_tick_3: Vec<QueueItem> = rows.iter().filter(|r| r.at_tick == 3).map(|r| r.item).collect();
        assert_eq!(at_tick_3, vec![QueueItem::Combatant(cid), QueueItem::Marker(MarkerId(0))]);
    }

    #[test]
    fn pending_markers_not_yet_started_are_included() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        apply(&mut battle, &BattleEvent::AddMarker { id: MarkerId(0), label: "Later".to_string(), source: cid, at_tick: 10, ticks: 2 }).unwrap();

        let rows = queue(&battle);
        assert!(rows.iter().any(|r| r.item == QueueItem::Marker(MarkerId(0))));
    }

    #[test]
    fn expired_markers_are_excluded() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        apply(&mut battle, &BattleEvent::AddMarker { id: MarkerId(0), label: "Gone".to_string(), source: cid, at_tick: 0, ticks: 1 }).unwrap();
        let guard = template(ActionKind::Guard).declare(Declaration::default());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();

        let rows = queue(&battle);
        assert!(!rows.iter().any(|r| r.item == QueueItem::Marker(MarkerId(0))));
    }
}
