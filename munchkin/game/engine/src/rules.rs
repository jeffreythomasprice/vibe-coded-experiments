//! The game rules engine — currently all stubs.
//!
//! The authoritative spec is `assets/processed/rules.md`. The submodules below
//! mirror the major systems described there; each is a placeholder to be filled
//! in incrementally. Nothing here is implemented yet.

use anyhow::Result;

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

/// Run the engine. Stub: just logs that it started and returns.
pub fn run() -> Result<()> {
    tracing::info!("engine started (stub) — game rules not yet implemented");
    Ok(())
}
