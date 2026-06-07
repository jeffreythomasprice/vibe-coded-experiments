//! The game rules engine — currently all stubs.
//!
//! The authoritative spec is `assets/processed/rules.md`. The submodules below
//! mirror the major systems described there; each is a placeholder to be filled
//! in incrementally. Nothing here is implemented yet.

use anyhow::Result;
use shared::config::OllamaConfig;

use crate::ai::{OllamaClient, OllamaRefereeAgent};

/// Turn structure: the four sequential phases (Kick Open the Door, Look for
/// Trouble, Loot the Room, Charity) and the transitions between them.
pub mod turn {
    // TODO: model the 4-phase turn loop and phase transitions.
}

/// Combat: strength calculation, multiple monsters, asking for help, the
/// response window, Run Away rolls, and resolving a kill.
pub mod combat {
    // TODO: implement combat resolution.
}

/// Items: equip/carry slots, Big-item rules, class/race/sex restrictions,
/// trading, and selling for levels.
pub mod items {
    // TODO: implement item slot management and trading.
}

/// Curses: immediate vs continuing curses, and their persistence through death.
pub mod curses {
    // TODO: implement curse application and continuing curses.
}

/// Level tracking and combat-strength modifiers (never drops below 1).
pub mod level {
    // TODO: implement level gain/loss and modifier stacking.
}

/// Death and respawn: body looting, card reset, and what persists.
pub mod death {
    // TODO: implement death, looting, and respawn.
}

/// Win detection: reaching level 10, which is only legal by killing a monster.
pub mod win {
    // TODO: implement win-condition checking.
}

/// Run the engine. Stub: constructs the AI referee, logs that it started, and
/// returns.
///
/// The referee agent is built here from the Ollama config so it is ready once
/// combat/action validation lands. It is a live stub today (see
/// [`crate::ai::referee`]); we deliberately do **not** block startup on Ollama
/// being reachable, since the AI backend is optional infrastructure.
pub fn run(ollama: &OllamaConfig) -> Result<()> {
    let referee_client = OllamaClient::from_config(ollama, &ollama.referee_model);
    let _referee = OllamaRefereeAgent::new(referee_client);
    tracing::info!(
        model = %ollama.referee_model,
        "referee agent ready (stub) — legality not yet enforced"
    );

    tracing::info!("engine started (stub) — game rules not yet implemented");
    Ok(())
}
