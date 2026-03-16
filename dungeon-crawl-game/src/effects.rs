use std::collections::VecDeque;
use std::fmt;

use bevy::prelude::*;
use rand::Rng;

use crate::dungeon::Dungeon;
use crate::schema_types::*;
use crate::types::{ActiveEffect, PendingDamageAllocation, PlayerInfo};

impl fmt::Display for StatName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatName::Strength => write!(f, "strength"),
            StatName::Speed => write!(f, "speed"),
            StatName::Intelligence => write!(f, "intelligence"),
            StatName::Sanity => write!(f, "sanity"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectSourceKind {
    Item,
    Event,
}

#[derive(Debug, Clone)]
pub struct EffectLog {
    pub player_idx: usize,
    pub source_kind: EffectSourceKind,
    pub source: String,
    pub source_description: String,
    pub description: String,
}

pub fn check_player_death(player: &PlayerInfo) -> bool {
    let s = &player.player.stats;
    s.strength <= 0 || s.speed <= 0 || s.intelligence <= 0 || s.sanity <= 0
}

pub fn apply_effect(
    effect: &Effect,
    trigger_player_idx: usize,
    trigger_room_pos: IVec2,
    players: &mut Vec<PlayerInfo>,
    dungeon: &Dungeon,
    rng: &mut impl Rng,
    source_kind: EffectSourceKind,
    source_name: &str,
    source_description: &str,
    allocations: &mut VecDeque<PendingDamageAllocation>,
) -> Vec<EffectLog> {
    let mut logs = Vec::new();

    // Trigger check
    match effect.trigger_type {
        EffectTriggerType::Always => {}
        EffectTriggerType::RandomRoll => {
            if !rng.random_bool(0.5) {
                return logs;
            }
        }
        EffectTriggerType::StatCheck => {
            let threshold = effect.result_amount.unwrap_or(10);
            let stat_name = effect.trigger_stat.as_ref().unwrap_or(&StatName::Strength);
            let stat_val = get_stat_value(&players[trigger_player_idx], stat_name);
            if stat_val < threshold {
                return logs;
            }
        }
    }

    tracing::info!(
        "Applying effect: {:?} target={:?} at ({},{})",
        effect.result_type, effect.target, trigger_room_pos.x, trigger_room_pos.y
    );

    let targets = resolve_targets(effect, trigger_player_idx, trigger_room_pos, players);

    let timing = &effect.timing_type;
    let turns = effect.timing_turns.unwrap_or(1) as u32;

    for idx in &targets {
        match *timing {
            EffectTimingType::Delayed => {
                let preview = format!(
                    "Delayed effect ({} turns): Will {}",
                    turns,
                    format_effect_preview(effect),
                );
                players[*idx].active_effects.push(ActiveEffect {
                    effect: effect.clone(),
                    source_room: trigger_room_pos,
                    source_kind,
                    source_name: source_name.to_string(),
                    source_description: source_description.to_string(),
                    remaining_turns: None,
                    delay_turns: Some(turns),
                });
                logs.push(EffectLog {
                    player_idx: *idx,
                    source_kind,
                    source: source_name.to_string(),
                    source_description: source_description.to_string(),
                    description: preview,
                });
            }
            EffectTimingType::Duration => {
                apply_result(effect, *idx, players, dungeon, rng, source_kind, source_name, source_description, &mut logs, allocations);
                players[*idx].active_effects.push(ActiveEffect {
                    effect: effect.clone(),
                    source_room: trigger_room_pos,
                    source_kind,
                    source_name: source_name.to_string(),
                    source_description: source_description.to_string(),
                    remaining_turns: Some(turns),
                    delay_turns: None,
                });
            }
            EffectTimingType::Immediate => {
                apply_result(effect, *idx, players, dungeon, rng, source_kind, source_name, source_description, &mut logs, allocations);
            }
        }

        if check_player_death(&players[*idx]) {
            players[*idx].dead = true;
            players[*idx].remaining_move = 0;
            logs.push(EffectLog {
                player_idx: *idx,
                source_kind,
                source: source_name.to_string(),
                source_description: source_description.to_string(),
                description: "Died!".to_string(),
            });
        }
    }

    logs
}

fn resolve_targets(
    effect: &Effect,
    trigger_player_idx: usize,
    trigger_room_pos: IVec2,
    players: &[PlayerInfo],
) -> Vec<usize> {
    match effect.target {
        EffectTarget::SelfKind => {
            if players[trigger_player_idx].dead {
                vec![]
            } else {
                vec![trigger_player_idx]
            }
        }
        EffectTarget::Room => players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.dead && p.location == trigger_room_pos)
            .map(|(i, _)| i)
            .collect(),
        EffectTarget::All => players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.dead)
            .map(|(i, _)| i)
            .collect(),
    }
}

fn apply_result(
    effect: &Effect,
    player_idx: usize,
    players: &mut Vec<PlayerInfo>,
    dungeon: &Dungeon,
    rng: &mut impl Rng,
    source_kind: EffectSourceKind,
    source_name: &str,
    source_description: &str,
    logs: &mut Vec<EffectLog>,
    allocations: &mut VecDeque<PendingDamageAllocation>,
) {
    let amount = effect.result_amount.unwrap_or(0);

    match effect.result_type {
        EffectResultType::ModifyStat => {
            let stat_name = effect.result_stat.as_ref().unwrap_or(&StatName::Strength);
            modify_stat(&mut players[player_idx], stat_name, amount);
            let desc = if amount >= 0 {
                format!("Gained {amount} {stat_name}")
            } else {
                format!("Lost {} {stat_name}", amount.abs())
            };
            logs.push(EffectLog {
                player_idx,
                source_kind,
                source: source_name.to_string(),
                source_description: source_description.to_string(),
                description: desc,
            });
        }
        EffectResultType::PhysicalDamage => {
            let total = amount.unsigned_abs() as u32;
            let sign = if amount < 0 { 1 } else { -1 };
            if total == 0 {
                logs.push(EffectLog {
                    player_idx,
                    source_kind,
                    source: source_name.to_string(),
                    source_description: source_description.to_string(),
                    description: "Took 0 physical damage".to_string(),
                });
            } else {
                allocations.push_back(PendingDamageAllocation {
                    player_idx,
                    total,
                    stat_a: StatName::Strength,
                    stat_b: StatName::Speed,
                    sign,
                    source_kind,
                    source_name: source_name.to_string(),
                    source_description: source_description.to_string(),
                });
            }
        }
        EffectResultType::MentalDamage => {
            let total = amount.unsigned_abs() as u32;
            let sign = if amount < 0 { 1 } else { -1 };
            if total == 0 {
                logs.push(EffectLog {
                    player_idx,
                    source_kind,
                    source: source_name.to_string(),
                    source_description: source_description.to_string(),
                    description: "Took 0 mental damage".to_string(),
                });
            } else {
                allocations.push_back(PendingDamageAllocation {
                    player_idx,
                    total,
                    stat_a: StatName::Intelligence,
                    stat_b: StatName::Sanity,
                    sign,
                    source_kind,
                    source_name: source_name.to_string(),
                    source_description: source_description.to_string(),
                });
            }
        }
        EffectResultType::Teleport => {
            let revealed: Vec<IVec2> = dungeon.grid.keys().copied().collect();
            if !revealed.is_empty() {
                let dest = revealed[rng.random_range(0..revealed.len())];
                players[player_idx].location = dest;
                let room_name = dungeon
                    .grid
                    .get(&dest)
                    .map(|r| r.room.name.as_str())
                    .unwrap_or("unknown");
                logs.push(EffectLog {
                    player_idx,
                    source_kind,
                    source: source_name.to_string(),
                    source_description: source_description.to_string(),
                    description: format!("Teleported to {room_name}"),
                });
            }
        }
        EffectResultType::LoseTurn => {
            players[player_idx].remaining_move = 0;
            logs.push(EffectLog {
                player_idx,
                source_kind,
                source: source_name.to_string(),
                source_description: source_description.to_string(),
                description: "Lost turn".to_string(),
            });
        }
    }
}

fn format_effect_preview(effect: &Effect) -> String {
    match effect.result_type {
        EffectResultType::ModifyStat => {
            let stat = effect.result_stat.as_ref().unwrap_or(&StatName::Strength);
            let amt = effect.result_amount.unwrap_or(0);
            if amt >= 0 {
                format!("gain {amt} {stat}")
            } else {
                format!("lose {} {stat}", amt.abs())
            }
        }
        EffectResultType::PhysicalDamage => {
            format!("take {} physical damage", effect.result_amount.unwrap_or(0))
        }
        EffectResultType::MentalDamage => {
            format!("take {} mental damage", effect.result_amount.unwrap_or(0))
        }
        EffectResultType::Teleport => "teleport".to_string(),
        EffectResultType::LoseTurn => "lose turn".to_string(),
    }
}

pub fn get_stat_value(player: &PlayerInfo, stat_name: &StatName) -> i64 {
    match stat_name {
        StatName::Strength => player.player.stats.strength,
        StatName::Speed => player.player.stats.speed,
        StatName::Intelligence => player.player.stats.intelligence,
        StatName::Sanity => player.player.stats.sanity,
    }
}

pub fn modify_stat(player: &mut PlayerInfo, stat_name: &StatName, amount: i64) {
    let stat = match stat_name {
        StatName::Strength => &mut player.player.stats.strength,
        StatName::Speed => &mut player.player.stats.speed,
        StatName::Intelligence => &mut player.player.stats.intelligence,
        StatName::Sanity => &mut player.player.stats.sanity,
    };
    *stat = (*stat + amount).max(0);
}

pub fn tick_effects_for_player(
    player_idx: usize,
    players: &mut Vec<PlayerInfo>,
    dungeon: &Dungeon,
    rng: &mut impl Rng,
    allocations: &mut VecDeque<PendingDamageAllocation>,
) -> Vec<EffectLog> {
    let mut logs = Vec::new();

    if players[player_idx].dead {
        return logs;
    }

    let mut i = 0;
    while i < players[player_idx].active_effects.len() {
        let ae = &mut players[player_idx].active_effects[i];

        if let Some(ref mut delay) = ae.delay_turns {
            if *delay > 0 {
                *delay -= 1;
                if *delay == 0 {
                    let effect = ae.effect.clone();
                    let kind = ae.source_kind;
                    let source = ae.source_name.clone();
                    let source_desc = ae.source_description.clone();
                    players[player_idx].active_effects.remove(i);
                    apply_result(&effect, player_idx, players, dungeon, rng, kind, &source, &source_desc, &mut logs, allocations);
                    if check_player_death(&players[player_idx]) {
                        players[player_idx].dead = true;
                        players[player_idx].remaining_move = 0;
                        logs.push(EffectLog {
                            player_idx,
                            source_kind: kind,
                            source,
                            source_description: source_desc,
                            description: "Died!".to_string(),
                        });
                    }
                    continue;
                }
            }
            i += 1;
            continue;
        }

        if let Some(ref mut remaining) = players[player_idx].active_effects[i].remaining_turns {
            if *remaining > 0 {
                *remaining -= 1;
                if *remaining == 0 {
                    let effect = players[player_idx].active_effects[i].effect.clone();
                    let kind = players[player_idx].active_effects[i].source_kind;
                    let source = players[player_idx].active_effects[i].source_name.clone();
                    let source_desc = players[player_idx].active_effects[i].source_description.clone();
                    players[player_idx].active_effects.remove(i);
                    if effect.result_type == EffectResultType::ModifyStat {
                        let amount = effect.result_amount.unwrap_or(0);
                        let stat_name = effect.result_stat.as_ref().unwrap_or(&StatName::Strength);
                        modify_stat(&mut players[player_idx], stat_name, -amount);
                        logs.push(EffectLog {
                            player_idx,
                            source_kind: kind,
                            source,
                            source_description: source_desc,
                            description: format!(
                                "{stat_name} effect expired (reversed {})",
                                amount.abs()
                            ),
                        });
                    }
                    continue;
                }
            }
        }

        i += 1;
    }

    // Remove inventory items whose active effects have all expired
    let active_item_names: Vec<String> = players[player_idx]
        .active_effects
        .iter()
        .filter(|ae| ae.source_kind == EffectSourceKind::Item)
        .map(|ae| ae.source_name.clone())
        .collect();
    players[player_idx]
        .inventory
        .retain(|item| active_item_names.contains(&item.name));

    logs
}

#[cfg(test)]
pub fn tick_effects(
    players: &mut Vec<PlayerInfo>,
    dungeon: &Dungeon,
    rng: &mut impl Rng,
    allocations: &mut VecDeque<PendingDamageAllocation>,
) -> Vec<EffectLog> {
    let mut logs = Vec::new();
    for player_idx in 0..players.len() {
        logs.extend(tick_effects_for_player(player_idx, players, dungeon, rng, allocations));
    }
    logs
}

pub fn format_effect_description(effect: &Effect) -> String {
    let result = match effect.result_type {
        EffectResultType::ModifyStat => {
            let stat = effect.result_stat.as_ref().unwrap_or(&StatName::Strength);
            let amt = effect.result_amount.unwrap_or(0);
            let sign = if amt >= 0 { "+" } else { "" };
            format!("{sign}{amt} {stat}")
        }
        EffectResultType::PhysicalDamage => {
            format!("{} physical damage", effect.result_amount.unwrap_or(0))
        }
        EffectResultType::MentalDamage => {
            format!("{} mental damage", effect.result_amount.unwrap_or(0))
        }
        EffectResultType::Teleport => "Teleport".to_string(),
        EffectResultType::LoseTurn => "Lose turn".to_string(),
    };

    match effect.timing_type {
        EffectTimingType::Duration => {
            format!("{result} ({} turns)", effect.timing_turns.unwrap_or(1))
        }
        EffectTimingType::Delayed => {
            format!("{result} (delayed {} turns)", effect.timing_turns.unwrap_or(1))
        }
        EffectTimingType::Immediate => result,
    }
}

// PartialEq needed for reverse-on-expiry check
impl PartialEq for EffectResultType {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn make_player(name: &str, stats: [i64; 4]) -> PlayerInfo {
        PlayerInfo {
            player: Player {
                name: name.to_string(),
                description: String::new(),
                stats: PlayerStats {
                    strength: stats[0],
                    speed: stats[1],
                    intelligence: stats[2],
                    sanity: stats[3],
                },
                color: None,
            },
            color: bevy::prelude::Color::WHITE,
            location: IVec2::ZERO,
            remaining_move: 3,
            destination: None,
            path: VecDeque::new(),
            active_effects: Vec::new(),
            inventory: Vec::new(),
            dead: false,
        }
    }

    fn make_effect(result_type: EffectResultType, amount: i64) -> Effect {
        Effect {
            trigger_type: EffectTriggerType::Always,
            trigger_stat: None,
            trigger_outcomes: None,
            result_type,
            result_stat: Some(StatName::Strength),
            result_amount: Some(amount),
            target: EffectTarget::SelfKind,
            timing_type: EffectTimingType::Immediate,
            timing_turns: None,
        }
    }

    #[test]
    fn modify_stat_applies() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::ModifyStat, -3);
        let logs = apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 7);
        assert!(!logs.is_empty());
    }

    #[test]
    fn stat_clamps_at_zero() {
        let mut players = vec![make_player("A", [2, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::ModifyStat, -5);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 0);
        assert!(players[0].dead);
    }

    #[test]
    fn death_on_zero_stat() {
        let mut players = vec![make_player("A", [1, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::ModifyStat, -1);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert!(players[0].dead);
        assert_eq!(players[0].remaining_move, 0);
    }

    #[test]
    fn random_roll_can_fail() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        // StepRng(u64::MAX, 0) generates values that make random_bool(0.5) return false
        let mut rng = rand::rngs::mock::StepRng::new(u64::MAX, 0);
        let mut effect = make_effect(EffectResultType::ModifyStat, -5);
        effect.trigger_type = EffectTriggerType::RandomRoll;
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 10);
    }

    #[test]
    fn lose_turn_zeroes_movement() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::LoseTurn, 0);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].remaining_move, 0);
    }

    #[test]
    fn target_all_affects_everyone() {
        let mut players = vec![
            make_player("A", [10, 10, 10, 10]),
            make_player("B", [10, 10, 10, 10]),
        ];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut effect = make_effect(EffectResultType::ModifyStat, -2);
        effect.target = EffectTarget::All;
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 8);
        assert_eq!(players[1].player.stats.strength, 8);
    }

    #[test]
    fn target_room_only_affects_colocated() {
        let mut players = vec![
            make_player("A", [10, 10, 10, 10]),
            make_player("B", [10, 10, 10, 10]),
        ];
        players[1].location = IVec2::new(1, 0);
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut effect = make_effect(EffectResultType::ModifyStat, -2);
        effect.target = EffectTarget::Room;
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 8);
        assert_eq!(players[1].player.stats.strength, 10);
    }

    #[test]
    fn delayed_effect_ticks() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut effect = make_effect(EffectResultType::ModifyStat, -3);
        effect.timing_type = EffectTimingType::Delayed;
        effect.timing_turns = Some(2);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 10);
        assert_eq!(players[0].active_effects.len(), 1);

        tick_effects(&mut players, &dungeon, &mut rng, &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 10); // delay=1
        tick_effects(&mut players, &dungeon, &mut rng, &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 7); // applied
        assert!(players[0].active_effects.is_empty());
    }

    #[test]
    fn duration_effect_reverses() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut effect = make_effect(EffectResultType::ModifyStat, 3);
        effect.timing_type = EffectTimingType::Duration;
        effect.timing_turns = Some(2);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 13);

        tick_effects(&mut players, &dungeon, &mut rng, &mut VecDeque::new()); // remaining=1
        assert_eq!(players[0].player.stats.strength, 13);
        tick_effects(&mut players, &dungeon, &mut rng, &mut VecDeque::new()); // remaining=0, reversed
        assert_eq!(players[0].player.stats.strength, 10);
        assert!(players[0].active_effects.is_empty());
    }

    #[test]
    fn dead_players_skipped() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        players[0].dead = true;
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut effect = make_effect(EffectResultType::ModifyStat, -5);
        effect.target = EffectTarget::All;
        let logs = apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng, EffectSourceKind::Item, "Test", "Test description", &mut VecDeque::new());
        assert_eq!(players[0].player.stats.strength, 10);
        assert!(logs.is_empty());
    }
}
