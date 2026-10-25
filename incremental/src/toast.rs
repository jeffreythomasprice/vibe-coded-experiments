use crate::hex::Point;
use crate::icon::Icon;
use crate::resource::{Amount, Resource};

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub resource: Resource,
    pub amount: Amount,
    pub icon: Icon,
    pub at: Point,
    pub age: f64,
}

impl Toast {
    pub fn label(&self) -> String {
        format!("+{}", self.amount)
    }
}

#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    pub fn push(&mut self, resource: Resource, amount: Amount, icon: Icon, at: Point) {
        self.items.push(Toast {
            id: self.next_id,
            resource,
            amount,
            icon,
            at,
            age: 0.0,
        });
        self.next_id += 1;
    }

    pub fn tick(&mut self, dt: f64, duration: f64) {
        for t in &mut self.items {
            t.age += dt;
        }
        self.items.retain(|t| t.age < duration);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Toast> {
        self.items.iter()
    }
}

pub fn opacity(age: f64, duration: f64, fade: f64) -> f64 {
    let fade = fade.clamp(0.0, duration);
    if fade == 0.0 {
        return if age < duration { 1.0 } else { 0.0 };
    }
    ((duration - age) / fade).clamp(0.0, 1.0)
}

pub fn offset(age: f64, duration: f64, rise: f64) -> f64 {
    if duration <= 0.0 {
        return rise;
    }
    rise * (age / duration).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_icon() -> Icon {
        Icon::generate(Resource::Water, 0)
    }

    fn push(toasts: &mut Toasts) {
        toasts.push(Resource::Water, 1, test_icon(), Point::new(0.0, 0.0));
    }

    #[test]
    fn opacity_holds_then_fades_linearly() {
        assert_eq!(opacity(0.0, 1.0, 0.5), 1.0);
        assert_eq!(opacity(0.5, 1.0, 0.5), 1.0);
        assert_eq!(opacity(0.75, 1.0, 0.5), 0.5);
        assert_eq!(opacity(1.0, 1.0, 0.5), 0.0);
        assert_eq!(opacity(2.0, 1.0, 0.5), 0.0);
    }

    #[test]
    fn opacity_handles_degenerate_fades() {
        assert_eq!(opacity(0.5, 1.0, 0.0), 1.0);
        assert_eq!(opacity(1.0, 1.0, 0.0), 0.0);
        assert_eq!(opacity(0.5, 1.0, 5.0), 0.5);
    }

    #[test]
    fn offset_rises_over_lifetime() {
        assert_eq!(offset(0.0, 2.0, 10.0), 0.0);
        assert_eq!(offset(1.0, 2.0, 10.0), 5.0);
        assert_eq!(offset(3.0, 2.0, 10.0), 10.0);
    }

    #[test]
    fn tick_expires_old_toasts() {
        let mut toasts = Toasts::default();
        push(&mut toasts);
        toasts.tick(0.6, 1.0);
        push(&mut toasts);
        toasts.tick(0.6, 1.0);
        assert_eq!(toasts.iter().count(), 1);
        toasts.tick(0.6, 1.0);
        assert_eq!(toasts.iter().count(), 0);
    }

    #[test]
    fn ids_are_unique() {
        let mut toasts = Toasts::default();
        push(&mut toasts);
        push(&mut toasts);
        let ids: Vec<u64> = toasts.iter().map(|t| t.id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn label_shows_amount() {
        let mut toasts = Toasts::default();
        toasts.push(Resource::Fire, 3, test_icon(), Point::new(0.0, 0.0));
        assert_eq!(toasts.iter().next().unwrap().label(), "+3");
    }
}
