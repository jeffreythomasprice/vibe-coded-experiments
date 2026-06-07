//! The vocabulary of what an agent is *asked* and what it *answers*.
//!
//! These enums are **illustrative placeholders** that mirror the moments in a
//! turn where play needs a choice (see `assets/processed/rules.md`). They will
//! churn heavily once the real engine and card model exist; for now they exist
//! so the agent stubs have concrete inputs and outputs to traffic in.

use serde::{Deserialize, Serialize};

/// Why an agent is being consulted. The two variants capture the fundamental
/// split: a decision the player is *required* to make, versus a chance to *jump
/// in* during another player's turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionRequest {
    /// The player must respond or accept the consequence (e.g. losing the
    /// combat). Play cannot continue until they answer.
    Mandatory(MandatoryContext),
    /// The player *may* act out of turn but is free to pass.
    Opportunity(OpportunityContext),
}

/// Situations that force a decision from the active player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MandatoryContext {
    /// A monster was revealed (e.g. by kicking open the door) and must be dealt
    /// with. `monster_id` is the card id; `monster_level` and `player_strength`
    /// summarise the combat math the agent should weigh.
    MonsterEncountered {
        monster_id: String,
        monster_level: i32,
        player_strength: i32,
    },
}

/// Situations where a non-active player may choose to intervene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpportunityContext {
    /// Another player is fighting a monster. This player may offer to help,
    /// play a card to hinder either side, or pass.
    CombatInProgress {
        active_player: usize,
        monster_id: String,
        monster_level: i32,
        /// The active player's current strength, including help already pledged.
        active_player_strength: i32,
    },
}

/// What an agent decides to do in response to a [`DecisionRequest`].
///
/// Not every variant is valid for every request — legality is the referee's
/// job (see [`super::referee`]). This is just the union of possible answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Fight the monster with current strength.
    Fight,
    /// Attempt to run away (a die roll, resolved by the engine).
    RunAway,
    /// Play a card from hand, referenced by its card id.
    PlayCard { card_id: String },
    /// Offer to help the active player in combat.
    OfferHelp,
    /// Play a card to hinder (e.g. buff the monster against the active player).
    Hinder { card_id: String },
    /// Decline an out-of-turn opportunity.
    Pass,
    /// Give up the mandatory decision and accept the consequence (e.g. the bad
    /// stuff from a monster you can't beat).
    Concede,
}

/// An action a player proposes to take, handed to the referee for a legality
/// ruling *before* the engine applies it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    /// Seat index (into `GameState::players`) of the player taking the action.
    pub player: usize,
    pub kind: ProposedActionKind,
}

/// The kinds of action the referee can be asked to validate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposedActionKind {
    /// Play a card from hand to the table or into a resolution.
    PlayCard { card_id: String },
    /// Hand a card to another player.
    GiveCard { card_id: String, to: usize },
    /// Make a combat decision (fight, run, etc.).
    CombatDecision { decision: Decision },
}

/// The referee's verdict on a [`ProposedAction`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruling {
    /// Whether the action is permitted by the rules.
    pub legal: bool,
    /// A human-readable justification (cited rule, or why it was rejected).
    pub reason: String,
}

impl Ruling {
    /// A permitting ruling.
    pub fn legal(reason: impl Into<String>) -> Self {
        Ruling {
            legal: true,
            reason: reason.into(),
        }
    }

    /// A rejecting ruling.
    pub fn illegal(reason: impl Into<String>) -> Self {
        Ruling {
            legal: false,
            reason: reason.into(),
        }
    }
}
