//! The limited-information AI **player** agent.
//!
//! A player agent decides moves knowing only its own hand and the public table
//! state — see [`PlayerView`]. This implementation is a **stub**: it builds a
//! placeholder prompt, asks Ollama best-effort, logs the raw reply, and then
//! returns a safe default decision regardless. Turning the raw reply into a real
//! [`Decision`] (and writing a real prompt) is the next step.

use anyhow::Result;

use crate::ai::client::OllamaClient;
use crate::ai::decision::{Decision, DecisionRequest};
use crate::ai::view::PlayerView;

/// An agent that decides a single player's moves from limited information.
///
/// Native `async fn` in trait (edition 2024). Used through the concrete
/// [`OllamaPlayerAgent`] for now; the trait marks the seam for alternative
/// implementations (a scripted bot in tests, a human relay, …).
pub trait PlayerAgent {
    /// Decide how to respond to `request`, given everything this player is
    /// allowed to know (`view`).
    async fn decide(&self, view: &PlayerView, request: &DecisionRequest) -> Result<Decision>;
}

/// A [`PlayerAgent`] backed by an Ollama model.
#[derive(Debug, Clone)]
pub struct OllamaPlayerAgent {
    client: OllamaClient,
}

impl OllamaPlayerAgent {
    /// Wrap an [`OllamaClient`] (already bound to the player model).
    pub fn new(client: OllamaClient) -> Self {
        OllamaPlayerAgent { client }
    }
}

impl PlayerAgent for OllamaPlayerAgent {
    async fn decide(&self, view: &PlayerView, request: &DecisionRequest) -> Result<Decision> {
        // TODO: build a real prompt describing the rules, this player's view,
        // and the available choices, then parse the reply into a `Decision`.
        let prompt = stub_prompt(view, request);

        match self.client.generate(&prompt).await {
            Ok(reply) => {
                tracing::debug!(
                    seat = view.seat,
                    model = self.client.model(),
                    %reply,
                    "ollama player reply (stub: not yet parsed)"
                );
            }
            Err(err) => {
                // Best-effort: a missing/unhappy Ollama must not stall play.
                tracing::warn!(
                    seat = view.seat,
                    error = %err,
                    "ollama player call failed; using default decision"
                );
            }
        }

        Ok(default_decision(request))
    }
}

/// Placeholder prompt. Intentionally minimal — real prompt engineering is TODO.
fn stub_prompt(view: &PlayerView, request: &DecisionRequest) -> String {
    format!(
        "You are the Munchkin player in seat {}. Decide what to do.\n\
         Your view: {view:?}\n\
         Request: {request:?}",
        view.seat
    )
}

/// The safe fallback while the agent is stubbed: never act out of turn, and
/// concede mandatory decisions rather than make an unvalidated move.
fn default_decision(request: &DecisionRequest) -> Decision {
    match request {
        DecisionRequest::Mandatory(_) => Decision::Concede,
        DecisionRequest::Opportunity(_) => Decision::Pass,
    }
}
