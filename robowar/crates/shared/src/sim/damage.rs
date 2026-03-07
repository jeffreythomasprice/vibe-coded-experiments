use crate::config::constants::SimConstants;

pub fn projectile_damage(projectile_damage: i32, target_armor: i32) -> i32 {
    (projectile_damage - target_armor).max(1)
}

pub fn wall_collision_damage(speed: f32, constants: &SimConstants) -> i32 {
    ((speed * constants.wall_damage_factor) as i32).max(0)
}

pub fn robot_collision_damage(closing_speed: f32, constants: &SimConstants) -> i32 {
    ((closing_speed * constants.robot_collision_damage_factor) as i32).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectile_damage_normal() {
        assert_eq!(projectile_damage(10, 3), 7);
    }

    #[test]
    fn projectile_damage_high_armor_clamps_to_one() {
        assert_eq!(projectile_damage(5, 10), 1);
    }

    #[test]
    fn projectile_damage_equal_armor_clamps_to_one() {
        assert_eq!(projectile_damage(5, 5), 1);
    }

    #[test]
    fn projectile_damage_zero_armor() {
        assert_eq!(projectile_damage(8, 0), 8);
    }

    #[test]
    fn wall_collision_damage_normal() {
        let c = SimConstants::default();
        assert_eq!(wall_collision_damage(10.0, &c), 5);
    }

    #[test]
    fn wall_collision_damage_zero_speed() {
        let c = SimConstants::default();
        assert_eq!(wall_collision_damage(0.0, &c), 0);
    }

    #[test]
    fn wall_collision_damage_negative_speed_clamps_to_zero() {
        let c = SimConstants::default();
        assert_eq!(wall_collision_damage(-5.0, &c), 0);
    }

    #[test]
    fn wall_collision_damage_low_speed_truncates() {
        let c = SimConstants::default();
        assert_eq!(wall_collision_damage(1.0, &c), 0);
    }

    #[test]
    fn robot_collision_damage_normal() {
        let c = SimConstants::default();
        assert_eq!(robot_collision_damage(10.0, &c), 3);
    }

    #[test]
    fn robot_collision_damage_zero_speed() {
        let c = SimConstants::default();
        assert_eq!(robot_collision_damage(0.0, &c), 0);
    }

    #[test]
    fn robot_collision_damage_negative_speed_clamps_to_zero() {
        let c = SimConstants::default();
        assert_eq!(robot_collision_damage(-5.0, &c), 0);
    }

    #[test]
    fn robot_collision_damage_low_speed_truncates() {
        let c = SimConstants::default();
        assert_eq!(robot_collision_damage(1.0, &c), 0);
    }
}
