use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single user-authored note. Notes can be attached to the `Character`
/// itself or to any individual `BackgroundRef`, `CharmRef`, `SpellRef`, or
/// `Combo`. `created_at` is set on construction and never changes; `edit`
/// replaces the body and bumps `updated_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Note {
    pub fn new(body: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            body: body.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn edit(&mut self, body: impl Into<String>) {
        self.body = body.into();
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_both_timestamps_equal() {
        let n = Note::new("hello");
        assert_eq!(n.body, "hello");
        assert_eq!(n.created_at, n.updated_at);
    }

    #[test]
    fn edit_preserves_created_and_advances_updated() {
        let mut n = Note::new("first");
        let original_created = n.created_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        n.edit("second");
        assert_eq!(n.body, "second");
        assert_eq!(n.created_at, original_created);
        assert!(n.updated_at > original_created);
    }
}
