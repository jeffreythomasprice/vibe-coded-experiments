#[derive(Debug, Clone, PartialEq)]
pub enum DamageSource {
    Projectile { owner_id: u32 },
    WallCollision,
    RobotCollision { other_id: u32 },
}

pub trait MatchObserver {
    fn on_tick_start(&mut self, tick: u32);
    fn on_robot_state(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        heading: f32,
        turret_bearing: f32,
        hp: i32,
    );
    fn on_projectile_state(&mut self, id: u32, x: f32, y: f32, direction: f32);
    fn on_scan_ray(
        &mut self,
        robot_id: u32,
        origin_x: f32,
        origin_y: f32,
        direction: f32,
        distance: f32,
        hit: bool,
    );
    fn on_damage(&mut self, target_id: u32, damage: i32, source: DamageSource);
    fn on_destruction(&mut self, robot_id: u32);
    fn on_tick_end(&mut self, tick: u32);
    fn on_match_end(&mut self, winner: Option<u32>, ticks: u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingObserver {
        events: Vec<String>,
    }

    impl MatchObserver for RecordingObserver {
        fn on_tick_start(&mut self, tick: u32) {
            self.events.push(format!("tick_start:{tick}"));
        }
        fn on_robot_state(
            &mut self,
            id: u32,
            _x: f32,
            _y: f32,
            _heading: f32,
            _turret_bearing: f32,
            _hp: i32,
        ) {
            self.events.push(format!("robot_state:{id}"));
        }
        fn on_projectile_state(&mut self, id: u32, _x: f32, _y: f32, _direction: f32) {
            self.events.push(format!("projectile_state:{id}"));
        }
        fn on_scan_ray(
            &mut self,
            robot_id: u32,
            _origin_x: f32,
            _origin_y: f32,
            _direction: f32,
            _distance: f32,
            hit: bool,
        ) {
            self.events.push(format!("scan_ray:{robot_id}:hit={hit}"));
        }
        fn on_damage(&mut self, target_id: u32, damage: i32, source: DamageSource) {
            self.events
                .push(format!("damage:{target_id}:{damage}:{source:?}"));
        }
        fn on_destruction(&mut self, robot_id: u32) {
            self.events.push(format!("destruction:{robot_id}"));
        }
        fn on_tick_end(&mut self, tick: u32) {
            self.events.push(format!("tick_end:{tick}"));
        }
        fn on_match_end(&mut self, winner: Option<u32>, ticks: u32) {
            self.events.push(format!("match_end:{winner:?}:{ticks}"));
        }
    }

    #[test]
    fn observer_records_events() {
        let mut obs = RecordingObserver::default();
        obs.on_tick_start(0);
        obs.on_robot_state(0, 100.0, 200.0, 0.0, 0.0, 50);
        obs.on_projectile_state(1, 150.0, 250.0, 1.5);
        obs.on_scan_ray(0, 100.0, 200.0, 0.0, 300.0, true);
        obs.on_damage(1, 10, DamageSource::Projectile { owner_id: 0 });
        obs.on_destruction(1);
        obs.on_tick_end(0);
        obs.on_match_end(Some(0), 1);

        assert_eq!(obs.events.len(), 8);
        assert_eq!(obs.events[0], "tick_start:0");
        assert_eq!(obs.events[7], "match_end:Some(0):1");
    }

    #[test]
    fn damage_source_variants() {
        let proj = DamageSource::Projectile { owner_id: 1 };
        let wall = DamageSource::WallCollision;
        let robot = DamageSource::RobotCollision { other_id: 2 };

        assert_ne!(proj, wall);
        assert_ne!(wall, robot);
        assert_eq!(proj.clone(), DamageSource::Projectile { owner_id: 1 });
    }
}
