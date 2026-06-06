//! Stubbed game data structures shared across clients.
//!
//! These are intentionally minimal placeholders — just enough vocabulary for
//! the engine and tui to start building against. The concrete card fields will
//! eventually mirror the curated dataset schema documented in
//! `assets/processed/README.md`, and the rules these types support are
//! described in `assets/processed/rules.md`.
//!
//! TODO: flesh these out as the engine grows. Expect heavy churn here.

use serde::{Deserialize, Serialize};

/// One of the four playable races. `Human` is the default (no race card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Race {
    Human,
    Elf,
    Dwarf,
    Halfling,
}

/// One of the four playable classes. A player with no class card has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Warrior,
    Wizard,
    Thief,
    Cleric,
}

/// Player sex — relevant only to sex-restricted items and a few monster bonuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Male,
    Female,
}

/// Broad category of a card, used to drive timing/legality rules.
///
/// TODO: this will likely grow subtype data (monster level, item bonus, etc.)
/// once cards are modelled properly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardKind {
    Monster,
    Curse,
    Race,
    Class,
    MonsterEnhancer,
    Item,
    GoUpALevel,
    OneShot,
    Other,
}

/// A single card, referenced by its id in the curated dataset
/// (`assets/processed/cards.toml`).
///
/// TODO: carry the actual card fields (title, body, bonuses, …) rather than
/// just an id + kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub kind: CardKind,
}

/// A player's full state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    pub level: u8,
    pub race: Race,
    pub class: Option<Class>,
    pub sex: Sex,
    /// Cards held in hand (no effect until played).
    pub hand: Vec<Card>,
    /// Cards played to the table (items in use, continuing curses, etc.).
    pub in_play: Vec<Card>,
    /// Whether this player's character is currently dead (awaiting respawn).
    pub dead: bool,
}

/// The overall state of a game in progress.
///
/// TODO: model decks, discard piles, the active combat, turn phase, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<Player>,
    /// Index into `players` of the player whose turn it is.
    pub active_player: usize,
}
