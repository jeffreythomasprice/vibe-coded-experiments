use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::sim::observer::{DamageSource, MatchObserver};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchReplay {
    pub arena_width: f32,
    pub arena_height: f32,
    pub robot_names: Vec<String>,
    pub ticks: Vec<TickSnapshot>,
    pub winner: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickSnapshot {
    pub tick: u32,
    pub robots: Vec<RobotSnapshot>,
    pub projectiles: Vec<ProjectileSnapshot>,
    pub events: Vec<TickEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotSnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub heading: f32,
    pub turret_bearing: f32,
    pub hp: i32,
    pub alive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectileSnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub direction: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TickEvent {
    Damage { target_id: u32, damage: i32 },
    Destruction { robot_id: u32 },
    Fire { robot_id: u32, x: f32, y: f32, direction: f32 },
}

impl MatchReplay {
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let replay: Self = serde_json::from_str(&json)?;
        Ok(replay)
    }
}

pub struct ReplayRecorder {
    arena_width: f32,
    arena_height: f32,
    robot_names: Vec<String>,
    ticks: Vec<TickSnapshot>,
    current_tick: Option<TickSnapshot>,
    winner: Option<u32>,
    destroyed_robots: Vec<u32>,
}

impl ReplayRecorder {
    pub fn new(arena_width: f32, arena_height: f32, robot_names: Vec<String>) -> Self {
        Self {
            arena_width,
            arena_height,
            robot_names,
            ticks: Vec::new(),
            current_tick: None,
            winner: None,
            destroyed_robots: Vec::new(),
        }
    }

    pub fn into_replay(self) -> MatchReplay {
        MatchReplay {
            arena_width: self.arena_width,
            arena_height: self.arena_height,
            robot_names: self.robot_names,
            ticks: self.ticks,
            winner: self.winner,
        }
    }
}

impl MatchObserver for ReplayRecorder {
    fn on_tick_start(&mut self, tick: u32) {
        self.current_tick = Some(TickSnapshot {
            tick,
            robots: Vec::new(),
            projectiles: Vec::new(),
            events: Vec::new(),
        });
    }

    fn on_robot_state(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        heading: f32,
        turret_bearing: f32,
        hp: i32,
    ) {
        if let Some(ref mut tick) = self.current_tick {
            tick.robots.push(RobotSnapshot {
                id,
                x,
                y,
                heading,
                turret_bearing,
                hp,
                alive: !self.destroyed_robots.contains(&id),
            });
        }
    }

    fn on_projectile_state(&mut self, id: u32, x: f32, y: f32, direction: f32) {
        if let Some(ref mut tick) = self.current_tick {
            tick.projectiles.push(ProjectileSnapshot {
                id,
                x,
                y,
                direction,
            });
        }
    }

    fn on_scan_ray(
        &mut self,
        _robot_id: u32,
        _origin_x: f32,
        _origin_y: f32,
        _direction: f32,
        _distance: f32,
        _hit: bool,
    ) {
        // Scan rays are not recorded in the replay format
    }

    fn on_damage(&mut self, target_id: u32, damage: i32, _source: DamageSource) {
        if let Some(ref mut tick) = self.current_tick {
            tick.events.push(TickEvent::Damage { target_id, damage });
        }
    }

    fn on_destruction(&mut self, robot_id: u32) {
        self.destroyed_robots.push(robot_id);
        if let Some(ref mut tick) = self.current_tick {
            tick.events.push(TickEvent::Destruction { robot_id });
        }
    }

    fn on_tick_end(&mut self, _tick: u32) {
        if let Some(tick) = self.current_tick.take() {
            self.ticks.push(tick);
        }
    }

    fn on_match_end(&mut self, winner: Option<u32>, _ticks: u32) {
        self.winner = winner;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_recorder() -> ReplayRecorder {
        ReplayRecorder::new(
            800.0,
            600.0,
            vec!["Bot1".to_string(), "Bot2".to_string()],
        )
    }

    #[test]
    fn recorder_captures_tick() {
        let mut rec = make_recorder();

        rec.on_tick_start(0);
        rec.on_robot_state(0, 100.0, 200.0, 0.0, 0.0, 50);
        rec.on_robot_state(1, 500.0, 400.0, 180.0, 10.0, 50);
        rec.on_projectile_state(0, 150.0, 200.0, 0.5);
        rec.on_tick_end(0);

        let replay = rec.into_replay();
        assert_eq!(replay.ticks.len(), 1);
        assert_eq!(replay.ticks[0].tick, 0);
        assert_eq!(replay.ticks[0].robots.len(), 2);
        assert_eq!(replay.ticks[0].projectiles.len(), 1);
        assert_eq!(replay.ticks[0].robots[0].x, 100.0);
        assert_eq!(replay.ticks[0].robots[1].heading, 180.0);
        assert_eq!(replay.ticks[0].projectiles[0].direction, 0.5);
    }

    #[test]
    fn recorder_captures_events() {
        let mut rec = make_recorder();

        rec.on_tick_start(0);
        rec.on_robot_state(0, 100.0, 200.0, 0.0, 0.0, 50);
        rec.on_robot_state(1, 500.0, 400.0, 180.0, 0.0, 50);
        rec.on_damage(1, 10, DamageSource::Projectile { owner_id: 0 });
        rec.on_destruction(1);
        rec.on_tick_end(0);
        rec.on_match_end(Some(0), 1);

        let replay = rec.into_replay();
        assert_eq!(replay.winner, Some(0));
        assert_eq!(replay.ticks[0].events.len(), 2);
        match &replay.ticks[0].events[0] {
            TickEvent::Damage { target_id, damage } => {
                assert_eq!(*target_id, 1);
                assert_eq!(*damage, 10);
            }
            _ => panic!("expected Damage event"),
        }
        match &replay.ticks[0].events[1] {
            TickEvent::Destruction { robot_id } => assert_eq!(*robot_id, 1),
            _ => panic!("expected Destruction event"),
        }
    }

    #[test]
    fn recorder_tracks_destroyed_robots() {
        let mut rec = make_recorder();

        rec.on_tick_start(0);
        rec.on_robot_state(0, 100.0, 200.0, 0.0, 0.0, 50);
        rec.on_robot_state(1, 500.0, 400.0, 180.0, 0.0, 1);
        rec.on_damage(1, 10, DamageSource::Projectile { owner_id: 0 });
        rec.on_destruction(1);
        rec.on_tick_end(0);

        // Next tick: robot 1 should show as not alive
        rec.on_tick_start(1);
        rec.on_robot_state(0, 100.0, 200.0, 0.0, 0.0, 50);
        rec.on_robot_state(1, 500.0, 400.0, 180.0, 0.0, 0);
        rec.on_tick_end(1);

        let replay = rec.into_replay();
        assert!(replay.ticks[0].robots[1].alive);
        assert!(!replay.ticks[1].robots[1].alive);
    }

    #[test]
    fn recorder_multiple_ticks() {
        let mut rec = make_recorder();

        for t in 0..5 {
            rec.on_tick_start(t);
            rec.on_robot_state(0, 100.0 + t as f32, 200.0, 0.0, 0.0, 50);
            rec.on_tick_end(t);
        }
        rec.on_match_end(None, 5);

        let replay = rec.into_replay();
        assert_eq!(replay.ticks.len(), 5);
        assert_eq!(replay.winner, None);
        assert_eq!(replay.ticks[3].robots[0].x, 103.0);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut rec = make_recorder();
        rec.on_tick_start(0);
        rec.on_robot_state(0, 100.0, 200.0, 45.0, 10.0, 50);
        rec.on_robot_state(1, 500.0, 400.0, 180.0, -5.0, 40);
        rec.on_projectile_state(0, 200.0, 200.0, 0.78);
        rec.on_damage(1, 5, DamageSource::Projectile { owner_id: 0 });
        rec.on_tick_end(0);
        rec.on_match_end(Some(0), 1);

        let replay = rec.into_replay();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_replay.json");

        replay.save(&path).unwrap();
        let loaded = MatchReplay::load(&path).unwrap();

        assert_eq!(loaded.arena_width, 800.0);
        assert_eq!(loaded.arena_height, 600.0);
        assert_eq!(loaded.robot_names, vec!["Bot1", "Bot2"]);
        assert_eq!(loaded.winner, Some(0));
        assert_eq!(loaded.ticks.len(), 1);
        assert_eq!(loaded.ticks[0].robots.len(), 2);
        assert_eq!(loaded.ticks[0].robots[0].x, 100.0);
        assert_eq!(loaded.ticks[0].robots[0].heading, 45.0);
        assert_eq!(loaded.ticks[0].projectiles.len(), 1);
        assert_eq!(loaded.ticks[0].events.len(), 1);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = MatchReplay::load(Path::new("/tmp/nonexistent_replay_12345.json"));
        assert!(result.is_err());
    }

    #[test]
    fn replay_metadata() {
        let replay = MatchReplay {
            arena_width: 1024.0,
            arena_height: 768.0,
            robot_names: vec!["Alpha".to_string(), "Beta".to_string()],
            ticks: Vec::new(),
            winner: None,
        };
        assert_eq!(replay.arena_width, 1024.0);
        assert_eq!(replay.arena_height, 768.0);
        assert_eq!(replay.robot_names.len(), 2);
    }
}
