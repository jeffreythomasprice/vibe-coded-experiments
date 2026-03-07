pub struct Projectile {
    pub id: u32,
    pub position: (f32, f32),
    pub direction: f32,
    pub speed: f32,
    pub damage: i32,
    pub owner_id: u32,
    pub lifetime_remaining: u32,
}

impl Projectile {
    pub fn advance(&mut self) {
        let dx = self.direction.cos() * self.speed;
        let dy = self.direction.sin() * self.speed;
        self.position.0 += dx;
        self.position.1 += dy;
        self.lifetime_remaining = self.lifetime_remaining.saturating_sub(1);
    }

    pub fn is_expired(&self) -> bool {
        self.lifetime_remaining == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn make_projectile(direction: f32, speed: f32, lifetime: u32) -> Projectile {
        Projectile {
            id: 1,
            position: (0.0, 0.0),
            direction,
            speed,
            damage: 10,
            owner_id: 0,
            lifetime_remaining: lifetime,
        }
    }

    #[test]
    fn advance_moves_along_direction() {
        let mut p = make_projectile(0.0, 5.0, 10);
        p.advance();
        assert!((p.position.0 - 5.0).abs() < 1e-5);
        assert!(p.position.1.abs() < 1e-5);
        assert_eq!(p.lifetime_remaining, 9);
    }

    #[test]
    fn advance_moves_at_angle() {
        let mut p = make_projectile(PI / 2.0, 3.0, 5);
        p.advance();
        assert!(p.position.0.abs() < 1e-5);
        assert!((p.position.1 - 3.0).abs() < 1e-5);
    }

    #[test]
    fn advance_accumulates_position() {
        let mut p = make_projectile(0.0, 2.0, 10);
        p.advance();
        p.advance();
        p.advance();
        assert!((p.position.0 - 6.0).abs() < 1e-5);
        assert_eq!(p.lifetime_remaining, 7);
    }

    #[test]
    fn advance_lifetime_saturates_at_zero() {
        let mut p = make_projectile(0.0, 1.0, 1);
        p.advance();
        assert_eq!(p.lifetime_remaining, 0);
        p.advance();
        assert_eq!(p.lifetime_remaining, 0);
    }

    #[test]
    fn is_expired_when_lifetime_zero() {
        let p = make_projectile(0.0, 1.0, 0);
        assert!(p.is_expired());
    }

    #[test]
    fn is_not_expired_when_lifetime_remaining() {
        let p = make_projectile(0.0, 1.0, 5);
        assert!(!p.is_expired());
    }

    #[test]
    fn advance_then_expired() {
        let mut p = make_projectile(0.0, 1.0, 2);
        assert!(!p.is_expired());
        p.advance();
        assert!(!p.is_expired());
        p.advance();
        assert!(p.is_expired());
    }
}
