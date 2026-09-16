//! Naming rules shared by the client (which sanitizes what a player types before showing it
//! optimistically) and the server (which is the actual authority on both a player's display name
//! and a room's name).

use serde::{Deserialize, Serialize};

/// A player's own display label passes through here before it's stored anywhere — a name is
/// replicated to every member of a room on every change, so an unbounded one makes one member's
/// typing everyone else's bandwidth. Not attempting to catch anything more than length: this is a
/// display label, not a security boundary.
pub const MAX_NAME_LEN: usize = 40;

/// Trims and truncates to `MAX_NAME_LEN` characters. Never rejects outright — an empty result is
/// the caller's own problem to guard against (the server drops an empty `Rename`; the client
/// disables its own submit button on one).
pub fn sanitize_name(name: &str) -> String {
    let trimmed = name.trim();
    match trimmed.char_indices().nth(MAX_NAME_LEN) {
        Some((end, _)) => trimmed[..end].to_string(),
        None => trimmed.to_string(),
    }
}

pub const MAX_ROOM_NAME_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum RoomNameError {
    #[error("room names can't be empty")]
    Empty,
    #[error("room names can be at most {MAX_ROOM_NAME_LEN} characters")]
    TooLong,
}

/// The canonical form of a room name used as the DynamoDB partition key and for lookups: trimmed
/// and lowercased, so "Alice's Game" and "alice's game" are the same room. Whatever the creator
/// actually typed is kept separately (as a room's `display_name`) for the UI to show verbatim.
pub fn room_key(name: &str) -> Result<String, RoomNameError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(RoomNameError::Empty);
    }
    if trimmed.chars().count() > MAX_ROOM_NAME_LEN {
        return Err(RoomNameError::TooLong);
    }
    Ok(trimmed.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_trims_and_truncates() {
        assert_eq!(sanitize_name("  Alice  "), "Alice");
        let long = "a".repeat(MAX_NAME_LEN + 10);
        assert_eq!(sanitize_name(&long).chars().count(), MAX_NAME_LEN);
    }

    #[test]
    fn sanitize_name_allows_empty() {
        assert_eq!(sanitize_name("   "), "");
    }

    #[test]
    fn room_key_trims_and_lowercases() {
        assert_eq!(room_key("  Alice's Game  ").unwrap(), "alice's game");
    }

    #[test]
    fn room_key_rejects_empty() {
        assert_eq!(room_key("   "), Err(RoomNameError::Empty));
    }

    #[test]
    fn room_key_rejects_too_long() {
        let long = "a".repeat(MAX_ROOM_NAME_LEN + 1);
        assert_eq!(room_key(&long), Err(RoomNameError::TooLong));
    }

    #[test]
    fn room_key_accepts_the_boundary_length() {
        let boundary = "a".repeat(MAX_ROOM_NAME_LEN);
        assert!(room_key(&boundary).is_ok());
    }
}
