//! The full-information **referee** / "rules lawyer" agent.
//!
//! Unlike a player agent, the referee sees the entire
//! [`GameState`](shared::model::GameState) — including every player's private
//! hand — and rules on whether a [`ProposedAction`] is legal. This
//! implementation is a **stub**: it builds a placeholder prompt (with all hands
//! visible), asks Ollama best-effort, logs the raw reply, and then returns a
//! permissive ruling so nothing is blocked while the real rules engine is
//! unbuilt. Parsing the reply into a real [`Ruling`] is the next step.

use anyhow::Result;
use shared::model::GameState;

use crate::ai::client::OllamaClient;
use crate::ai::decision::{ProposedAction, Ruling};

/// An agent that rules on the legality of proposed actions with full knowledge
/// of the game state.
pub trait RefereeAgent {
    /// Rule on whether `action` is legal in `state`.
    async fn rule(&self, state: &GameState, action: &ProposedAction) -> Result<Ruling>;
}

/// A [`RefereeAgent`] backed by an Ollama model.
#[derive(Debug, Clone)]
pub struct OllamaRefereeAgent {
    client: OllamaClient,
}

impl OllamaRefereeAgent {
    /// Wrap an [`OllamaClient`] (already bound to the referee model).
    pub fn new(client: OllamaClient) -> Self {
        OllamaRefereeAgent { client }
    }
}

impl RefereeAgent for OllamaRefereeAgent {
    async fn rule(&self, state: &GameState, action: &ProposedAction) -> Result<Ruling> {
        // TODO: build a real prompt citing the rules and the full state (hands
        // included), then parse the reply into a `Ruling`.
        let prompt = stub_prompt(state, action);

        match self.client.generate(&prompt).await {
            Ok(reply) => {
                tracing::debug!(
                    model = self.client.model(),
                    %reply,
                    "ollama referee reply (stub: not yet parsed)"
                );
            }
            Err(err) => {
                // Best-effort: a missing/unhappy Ollama must not block actions.
                tracing::warn!(
                    error = %err,
                    "ollama referee call failed; allowing action by default"
                );
            }
        }

        // Permissive default: don't block play while legality isn't enforced.
        Ok(Ruling::legal("stub: rules not yet enforced"))
    }
}

/// Placeholder prompt. The referee deliberately gets the full state — every
/// player's hand — which is exactly what distinguishes it from a player agent.
fn stub_prompt(state: &GameState, action: &ProposedAction) -> String {
    format!(
        "You are the Munchkin rules referee with full knowledge of the game.\n\
         Full state (all hands visible): {state:?}\n\
         Proposed action: {action:?}\n\
         Is this action legal?"
    )
}
