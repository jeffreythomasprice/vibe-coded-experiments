use std::path::PathBuf;

use crate::character::{Caste, Character};
use crate::error::ValidationReport;
use crate::rules::database::{BackgroundEntry, CharmEntry, SpellEntry};
use crate::ui::pickers::background_picker::BackgroundPickerState;
use crate::ui::pickers::charm_picker::CharmPickerState;
use crate::ui::pickers::spell_picker::SpellPickerState;

/// What the binary asked the UI to do on launch.
pub enum StartupAction {
    /// Open with no document; user goes through File → New / Open.
    Empty,
    /// Open with this file loaded; if loading fails, surface an error toast
    /// and start empty.
    OpenFile(PathBuf),
}

/// A user action that's waiting on the confirm-discard modal to clear before
/// it can proceed (because the current document is dirty).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PendingAction {
    NewDocument,
    OpenExisting,
    Quit,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

pub struct AppState {
    pub character: Character,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,

    // Wired up by Phase 6 (validation dock); phase-1 plumbing leaves these
    // resident on the struct so section render fns can call `mark_dirty`.
    #[allow(dead_code)]
    pub last_validation: ValidationReport,
    pub validation_dirty: bool,

    pub validation_panel_collapsed: bool,
    pub sidebar_collapsed: bool,

    pub charm_picker: Option<CharmPickerState>,
    pub spell_picker: Option<SpellPickerState>,
    pub background_picker: Option<BackgroundPickerState>,

    /// "Edit details…" modal state for a `BackgroundRef::Custom` entry on
    /// the character. Holds the index into `character.backgrounds` and a
    /// working copy of the entry being edited. Cleared on Save/Cancel.
    pub editing_custom_background: Option<(usize, BackgroundEntry)>,
    pub editing_custom_charm: Option<(usize, CharmEntry)>,
    pub editing_custom_spell: Option<(usize, SpellEntry)>,

    pub pending_action: Option<PendingAction>,
    pub status_message: Option<(StatusKind, String)>,
    pub last_dir: Option<PathBuf>,
    pub last_title: String,
}

impl AppState {
    pub fn new_empty() -> Self {
        let character = blank_character();
        Self {
            character,
            file_path: None,
            dirty: false,
            last_validation: ValidationReport::new(),
            validation_dirty: true,
            validation_panel_collapsed: false,
            sidebar_collapsed: false,
            charm_picker: None,
            spell_picker: None,
            background_picker: None,
            editing_custom_background: None,
            editing_custom_charm: None,
            editing_custom_spell: None,
            pending_action: None,
            status_message: None,
            last_dir: None,
            last_title: String::new(),
        }
    }

    #[allow(dead_code)]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.validation_dirty = true;
    }
}

/// A minimal starting `Character` for File → New: a blank Dawn Solar with no
/// name. The user picks the caste / favored abilities / etc. via the Identity
/// section.
pub fn blank_character() -> Character {
    Character::new_blank_solar("", Caste::Dawn)
}
