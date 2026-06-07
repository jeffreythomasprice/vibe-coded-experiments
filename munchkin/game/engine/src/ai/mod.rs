//! AI agents that consult an Ollama-hosted LLM to drive non-human play.
//!
//! There are **two distinct kinds of agent**, distinguished by *how much they
//! know* and *what they decide*:
//!
//! - **Player agents** ([`player`]) act for an AI-controlled seat with **limited
//!   information** — their own hand plus the public table state (other players'
//!   levels, cards in play, and *hand sizes*, but not the contents of those
//!   hands). They are consulted both for **mandatory** decisions (a monster was
//!   kicked open and the player must fight / run / play a card / concede) and
//!   for **out-of-turn opportunities** (someone else is in combat and this
//!   player may offer help, play a hindering card, or pass). See
//!   [`decision::DecisionRequest`].
//!
//! - The **referee** / "rules lawyer" ([`referee`]) is consulted with **full
//!   information** — the entire [`GameState`](shared::model::GameState),
//!   including every player's private hand — to rule whether a
//!   [`ProposedAction`](decision::ProposedAction) is legal.
//!
//! The information asymmetry is enforced in [`view`]: player agents only ever
//! receive a redacted [`PlayerView`](view::PlayerView), while the referee takes
//! the raw `GameState`.
//!
//! Everything below the [`client`] is a **stub**: the Ollama client is real and
//! makes live HTTP calls, but the agents build placeholder prompts and fall back
//! to safe default decisions (so the engine builds and runs whether or not an
//! Ollama server is reachable). Prompt construction and response parsing are
//! the obvious next steps.
//!
//! Most of this module is scaffolding the rest of the engine does not call yet
//! (only the referee is constructed, in `rules::run`), so unused-code lints are
//! allowed here until the rules engine wires the agents in.
#![allow(dead_code, unused_imports)]

pub mod client;
pub mod decision;
pub mod player;
pub mod referee;
pub mod view;

pub use client::OllamaClient;
pub use decision::{Decision, DecisionRequest, ProposedAction, Ruling};
pub use player::{OllamaPlayerAgent, PlayerAgent};
pub use referee::{OllamaRefereeAgent, RefereeAgent};
pub use view::{PlayerView, PublicPlayer, player_view};
