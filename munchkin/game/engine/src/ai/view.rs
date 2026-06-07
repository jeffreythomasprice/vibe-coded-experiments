//! The information-asymmetry layer between the full game state and what an AI
//! **player** is allowed to see.
//!
//! A player knows their own hand but only the *public* facts about everyone else
//! — their level, race/class/sex, cards in play, and how many cards they hold,
//! but **not which cards**. [`player_view`] performs that redaction. The referee
//! does not use anything here; it takes the raw
//! [`GameState`](shared::model::GameState) with all hands intact.

use serde::{Deserialize, Serialize};
use shared::model::{Card, Class, GameState, Player, Race, Sex};

/// The publicly visible facts about an opponent — everything except the
/// contents of their hand, which is reduced to a count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayer {
    pub name: String,
    pub level: u8,
    pub race: Race,
    pub class: Option<Class>,
    pub sex: Sex,
    /// How many cards this player holds — but **not** which ones.
    pub hand_size: usize,
    /// Cards played to the table, which are public information.
    pub in_play: Vec<Card>,
    /// Whether this player is currently dead (awaiting respawn).
    pub dead: bool,
}

impl PublicPlayer {
    /// Redact a [`Player`] down to what opponents may see.
    fn from_player(p: &Player) -> Self {
        PublicPlayer {
            name: p.name.clone(),
            level: p.level,
            race: p.race,
            class: p.class,
            sex: p.sex,
            hand_size: p.hand.len(),
            in_play: p.in_play.clone(),
            dead: p.dead,
        }
    }
}

/// What a single AI player is allowed to know: their own full state plus the
/// public view of everyone else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    /// This player's own seat index (into the original `GameState::players`).
    pub seat: usize,
    /// This player's own full state — hand included.
    pub me: Player,
    /// The public view of every *other* player, in seat order.
    pub others: Vec<PublicPlayer>,
    /// Seat index of the player whose turn it is.
    pub active_player: usize,
}

/// Build the limited-information view for the player at `seat`.
///
/// The seat's own [`Player`] is cloned in full (they can see their hand); every
/// other player is redacted to a [`PublicPlayer`] (hand reduced to a count).
/// This is the single function that enforces the player-agent information
/// boundary — agents never touch the raw `GameState`.
///
/// # Panics
/// Panics if `seat` is out of range for `state.players`.
pub fn player_view(state: &GameState, seat: usize) -> PlayerView {
    assert!(
        seat < state.players.len(),
        "seat {seat} out of range for {} players",
        state.players.len()
    );

    let others = state
        .players
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != seat)
        .map(|(_, p)| PublicPlayer::from_player(p))
        .collect();

    PlayerView {
        seat,
        me: state.players[seat].clone(),
        others,
        active_player: state.active_player,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::model::{CardKind, Race, Sex};

    fn card(id: &str) -> Card {
        Card {
            id: id.to_string(),
            kind: CardKind::Item,
        }
    }

    fn player(name: &str, hand: Vec<Card>) -> Player {
        Player {
            name: name.to_string(),
            level: 1,
            race: Race::Human,
            class: None,
            sex: Sex::Female,
            hand,
            in_play: Vec::new(),
            dead: false,
        }
    }

    #[test]
    fn redacts_other_hands_but_keeps_own() {
        let state = GameState {
            players: vec![
                player("me", vec![card("a"), card("b")]),
                player("them", vec![card("x"), card("y"), card("z")]),
            ],
            active_player: 0,
        };

        let view = player_view(&state, 0);

        // Own hand is preserved in full.
        assert_eq!(view.me.name, "me");
        assert_eq!(view.me.hand.len(), 2);

        // The opponent is reduced to a count — no card contents leak.
        assert_eq!(view.others.len(), 1);
        assert_eq!(view.others[0].name, "them");
        assert_eq!(view.others[0].hand_size, 3);
    }
}
