use bevy::prelude::*;
use rand::Rng;

use crate::dungeon::Dungeon;
use crate::schema_types::*;
use crate::types::{ActiveEffect, PlayerInfo};

#[derive(Debug, Clone)]
pub struct EffectLog {
    pub player_idx: usize,
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
            let stat_val = get_stat_value(&players[trigger_player_idx], effect.trigger_stat.as_deref());
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
                players[*idx].active_effects.push(ActiveEffect {
                    effect: effect.clone(),
                    source_room: trigger_room_pos,
                    remaining_turns: None,
                    delay_turns: Some(turns),
                });
                logs.push(EffectLog {
                    player_idx: *idx,
                    description: format!("Delayed effect attached to {}", players[*idx].player.name),
                });
            }
            EffectTimingType::Duration => {
                apply_result(effect, *idx, players, dungeon, rng, &mut logs);
                players[*idx].active_effects.push(ActiveEffect {
                    effect: effect.clone(),
                    source_room: trigger_room_pos,
                    remaining_turns: Some(turns),
                    delay_turns: None,
                });
            }
            EffectTimingType::Immediate => {
                apply_result(effect, *idx, players, dungeon, rng, &mut logs);
            }
        }

        if check_player_death(&players[*idx]) {
            players[*idx].dead = true;
            players[*idx].remaining_move = 0;
            logs.push(EffectLog {
                player_idx: *idx,
                description: format!("{} has died!", players[*idx].player.name),
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
    logs: &mut Vec<EffectLog>,
) {
    let amount = effect.result_amount.unwrap_or(0);
    let name = players[player_idx].player.name.clone();

    match effect.result_type {
        EffectResultType::ModifyStat => {
            let stat_name = effect.result_stat.as_deref().unwrap_or("strength");
            modify_stat(&mut players[player_idx], stat_name, amount);
            logs.push(EffectLog {
                player_idx,
                description: format!("{name}: {stat_name} modified by {amount}"),
            });
        }
        EffectResultType::PhysicalDamage => {
            let (str_dmg, spd_dmg) = split_damage(amount.unsigned_abs() as u32, rng);
            let sign = if amount < 0 { 1 } else { -1 };
            modify_stat(&mut players[player_idx], "strength", sign * str_dmg as i64);
            modify_stat(&mut players[player_idx], "speed", sign * spd_dmg as i64);
            logs.push(EffectLog {
                player_idx,
                description: format!("{name}: physical damage {amount} (str:{str_dmg} spd:{spd_dmg})"),
            });
        }
        EffectResultType::MentalDamage => {
            let (int_dmg, san_dmg) = split_damage(amount.unsigned_abs() as u32, rng);
            let sign = if amount < 0 { 1 } else { -1 };
            modify_stat(&mut players[player_idx], "intelligence", sign * int_dmg as i64);
            modify_stat(&mut players[player_idx], "sanity", sign * san_dmg as i64);
            logs.push(EffectLog {
                player_idx,
                description: format!("{name}: mental damage {amount} (int:{int_dmg} san:{san_dmg})"),
            });
        }
        EffectResultType::Teleport => {
            let revealed: Vec<IVec2> = dungeon.grid.keys().copied().collect();
            if !revealed.is_empty() {
                let dest = revealed[rng.random_range(0..revealed.len())];
                players[player_idx].location = dest;
                logs.push(EffectLog {
                    player_idx,
                    description: format!("{name}: teleported to ({}, {})", dest.x, dest.y),
                });
            }
        }
        EffectResultType::LoseTurn => {
            players[player_idx].remaining_move = 0;
            logs.push(EffectLog {
                player_idx,
                description: format!("{name}: lost turn"),
            });
        }
        EffectResultType::GainItem
        | EffectResultType::LoseItem
        | EffectResultType::ModifyItem
        | EffectResultType::ModifyRoom => {
            // not yet implemented
        }
    }
}

fn split_damage(total: u32, rng: &mut impl Rng) -> (u32, u32) {
    if total == 0 {
        return (0, 0);
    }
    let first = rng.random_range(0..=total);
    (first, total - first)
}

fn get_stat_value(player: &PlayerInfo, stat_name: Option<&str>) -> i64 {
    match stat_name.unwrap_or("strength") {
        "strength" => player.player.stats.strength,
        "speed" => player.player.stats.speed,
        "intelligence" => player.player.stats.intelligence,
        "sanity" => player.player.stats.sanity,
        _ => 0,
    }
}

fn modify_stat(player: &mut PlayerInfo, stat_name: &str, amount: i64) {
    let stat = match stat_name {
        "strength" => &mut player.player.stats.strength,
        "speed" => &mut player.player.stats.speed,
        "intelligence" => &mut player.player.stats.intelligence,
        "sanity" => &mut player.player.stats.sanity,
        _ => return,
    };
    *stat = (*stat + amount).max(0);
}

pub fn tick_effects(
    players: &mut Vec<PlayerInfo>,
    dungeon: &Dungeon,
    rng: &mut impl Rng,
) -> Vec<EffectLog> {
    let mut logs = Vec::new();

    for player_idx in 0..players.len() {
        if players[player_idx].dead {
            continue;
        }

        let mut i = 0;
        while i < players[player_idx].active_effects.len() {
            let ae = &mut players[player_idx].active_effects[i];

            if let Some(ref mut delay) = ae.delay_turns {
                if *delay > 0 {
                    *delay -= 1;
                    if *delay == 0 {
                        let effect = ae.effect.clone();
                        players[player_idx].active_effects.remove(i);
                        apply_result(&effect, player_idx, players, dungeon, rng, &mut logs);
                        if check_player_death(&players[player_idx]) {
                            players[player_idx].dead = true;
                            players[player_idx].remaining_move = 0;
                            logs.push(EffectLog {
                                player_idx,
                                description: format!("{} has died!", players[player_idx].player.name),
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
                        players[player_idx].active_effects.remove(i);
                        // Reverse the effect on expiry for ModifyStat
                        if effect.result_type == EffectResultType::ModifyStat {
                            let amount = effect.result_amount.unwrap_or(0);
                            let stat_name = effect.result_stat.as_deref().unwrap_or("strength");
                            modify_stat(&mut players[player_idx], stat_name, -amount);
                            logs.push(EffectLog {
                                player_idx,
                                description: format!(
                                    "{}: {} effect expired, reversed by {}",
                                    players[player_idx].player.name, stat_name, amount
                                ),
                            });
                        }
                        continue;
                    }
                }
            }

            i += 1;
        }
    }

    logs
}

pub fn format_effect_description(effect: &Effect) -> String {
    let result = match effect.result_type {
        EffectResultType::ModifyStat => {
            let stat = effect.result_stat.as_deref().unwrap_or("?");
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
        _ => "Unknown".to_string(),
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
            dead: false,
        }
    }

    fn make_effect(result_type: EffectResultType, amount: i64) -> Effect {
        Effect {
            trigger_type: EffectTriggerType::Always,
            trigger_stat: None,
            trigger_outcomes: None,
            result_type,
            result_stat: Some("strength".to_string()),
            result_amount: Some(amount),
            result_item_name: None,
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
        let logs = apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 7);
        assert!(!logs.is_empty());
    }

    #[test]
    fn stat_clamps_at_zero() {
        let mut players = vec![make_player("A", [2, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::ModifyStat, -5);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 0);
        assert!(players[0].dead);
    }

    #[test]
    fn death_on_zero_stat() {
        let mut players = vec![make_player("A", [1, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::ModifyStat, -1);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
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
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 10);
    }

    #[test]
    fn lose_turn_zeroes_movement() {
        let mut players = vec![make_player("A", [10, 10, 10, 10])];
        let dungeon = Dungeon::default();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let effect = make_effect(EffectResultType::LoseTurn, 0);
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
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
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
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
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
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
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 10);
        assert_eq!(players[0].active_effects.len(), 1);

        tick_effects(&mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 10); // delay=1
        tick_effects(&mut players, &dungeon, &mut rng);
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
        apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 13);

        tick_effects(&mut players, &dungeon, &mut rng); // remaining=1
        assert_eq!(players[0].player.stats.strength, 13);
        tick_effects(&mut players, &dungeon, &mut rng); // remaining=0, reversed
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
        let logs = apply_effect(&effect, 0, IVec2::ZERO, &mut players, &dungeon, &mut rng);
        assert_eq!(players[0].player.stats.strength, 10);
        assert!(logs.is_empty());
    }
}
