//! Row identifiers for `lib::db`'s tables.
//!
//! Plain `i64` newtypes wrapping SQLite's `INTEGER PRIMARY KEY` rowid — this
//! workspace has no `uuid` dependency, and a single-user local database file
//! has nothing to merge across machines that would call for one.
//! `#[serde(transparent)]` so each crosses IPC as a bare JSON number, safe up
//! to 2^53 (a JS/wasm `Number`) long before SQLite's own `i64` range matters.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl $name {
            pub fn new(id: i64) -> Self {
                Self(id)
            }

            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<i64> for $name {
            fn from(id: i64) -> Self {
                Self(id)
            }
        }
    };
}

id_type!(AgentConfigId, "Primary key of `agent_configs`.");
id_type!(ConversationId, "Primary key of `conversations`.");
id_type!(TurnId, "Primary key of `turns`.");
id_type!(MessageId, "Primary key of `messages`.");
id_type!(ThemeId, "Primary key of `themes`.");
id_type!(ProjectId, "Primary key of `projects`.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_as_bare_numbers() {
        assert_eq!(
            serde_json::to_value(ConversationId(7)).unwrap(),
            serde_json::json!(7)
        );
        let back: ConversationId = serde_json::from_value(serde_json::json!(7)).unwrap();
        assert_eq!(back, ConversationId(7));
    }

    #[test]
    fn display_matches_the_bare_integer() {
        assert_eq!(TurnId(42).to_string(), "42");
    }
}
