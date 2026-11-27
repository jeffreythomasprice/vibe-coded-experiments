//! Abstract persistent-state interface.
//!
//! This is the seam between the rest of the workspace and *how* state is stored.
//! It is deliberately free of any notion of files or paths: a backend might keep
//! things on disk, in a database, or in the cloud. The desktop shell's
//! file-system backend lives in the `emulator` crate and is the only
//! implementation for now.
//!
//! The interface is split into three focused traits — [`SettingsStore`],
//! [`RomLibrary`], and [`SaveStore`] — with [`PersistentStore`] as a convenience
//! supertrait for callers that want all three. State is addressed through opaque
//! identities ([`RomId`], [`SaveId`]) that the backend is free to interpret
//! however it likes; callers never construct paths.

use std::fmt;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

/// Opaque handle to a ROM in the library. The backend decides what the inner
/// string means (a file name, a database key, …); callers treat it as opaque.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RomId(pub String);

/// A ROM as advertised by the library: a stable [`RomId`] plus a human-facing
/// label suitable for display in a future ROM browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomEntry {
    pub id: RomId,
    pub name: String,
}

/// The identity a save belongs to. The host derives it from a loaded cartridge
/// (for example, its header title); the backend maps it to storage however it
/// likes. Like [`RomId`], the inner string is opaque to callers.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SaveId(pub String);

/// Which save state of a game is being addressed. A save state is a snapshot of
/// the whole machine; the kind says who writes it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StateSlot {
    /// Saved by the user under a name of their choosing.
    Manual(String),
    /// One of the rotating quicksaves, numbered from 1.
    Quick(u32),
    /// The single state the host writes on its own.
    Auto,
}

impl StateSlot {
    /// A human-facing name for the slot.
    pub fn label(&self) -> String {
        match self {
            StateSlot::Manual(name) => name.clone(),
            StateSlot::Quick(number) => format!("Quicksave {number}"),
            StateSlot::Auto => "Autosave".to_string(),
        }
    }
}

/// A save state as advertised by the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateEntry {
    pub slot: StateSlot,
    /// When it was last written.
    pub modified: SystemTime,
}

/// A picture of the screen at the moment a state was saved: 8-bit RGBA pixels,
/// row by row from the top left.
#[derive(Clone, PartialEq, Eq)]
pub struct Screenshot {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Screenshot {
    /// Fails unless `rgba` holds exactly `width * height` pixels, and there is at
    /// least one.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Screenshot, StorageError> {
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(rgba.len() as u64) || rgba.is_empty() {
            return Err(StorageError::Serialization(format!(
                "{} bytes is not a {width}x{height} RGBA picture",
                rgba.len()
            )));
        }
        Ok(Screenshot {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl fmt::Debug for Screenshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Screenshot({}x{})", self.width, self.height)
    }
}

/// Errors any storage backend may surface.
///
/// Kept intentionally coarse and backend-neutral: it names neither paths nor a
/// concrete IO/serialization type, so this crate stays free of `std::path`,
/// `toml`, and the like. A backend folds its own context into the message.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("i/o error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("configuration error: {0}")]
    Config(String),
}

/// Load and persist the backend-agnostic [`Settings`].
pub trait SettingsStore {
    fn load_settings(&self) -> Result<Settings, StorageError>;
    fn save_settings(&self, settings: &Settings) -> Result<(), StorageError>;
}

/// Enumerate the available ROMs and fetch a ROM's raw bytes.
pub trait RomLibrary {
    fn list_roms(&self) -> Result<Vec<RomEntry>, StorageError>;
    fn read_rom(&self, id: &RomId) -> Result<Vec<u8>, StorageError>;
}

/// Read and write per-cartridge save data: battery RAM and save states.
pub trait SaveStore {
    /// Read the battery-backed cartridge RAM (the classic `.sav`), returning
    /// `Ok(None)` when none exists yet.
    fn read_battery(&self, id: &SaveId) -> Result<Option<Vec<u8>>, StorageError>;
    fn write_battery(&self, id: &SaveId, data: &[u8]) -> Result<(), StorageError>;
    /// Read a save state, returning `Ok(None)` when the slot is empty.
    fn read_state(&self, id: &SaveId, slot: &StateSlot) -> Result<Option<Vec<u8>>, StorageError>;
    /// Write a save state, replacing the slot's state and screenshot. A
    /// screenshot that can't be stored doesn't fail the save.
    fn write_state(
        &self,
        id: &SaveId,
        slot: &StateSlot,
        data: &[u8],
        screenshot: Option<&Screenshot>,
    ) -> Result<(), StorageError>;
    /// The screenshot saved with a state, if there is a usable one.
    fn read_screenshot(
        &self,
        id: &SaveId,
        slot: &StateSlot,
    ) -> Result<Option<Screenshot>, StorageError>;
    /// Every save state belonging to `id`, in no particular order.
    fn list_states(&self, id: &SaveId) -> Result<Vec<StateEntry>, StorageError>;
}

/// Convenience supertrait for a backend that provides all three capabilities.
pub trait PersistentStore: SettingsStore + RomLibrary + SaveStore {}
