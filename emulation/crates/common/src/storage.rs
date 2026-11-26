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

/// Which save is being addressed for a given [`SaveId`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveSlot {
    /// The battery-backed cartridge RAM (the classic `.sav`).
    Battery,
    /// A named save state: a snapshot of the whole machine.
    State(String),
}

/// The name of the save state the host writes on its own. Like the quicksaves,
/// it is an ordinary state recognised by name.
pub const AUTOSAVE_NAME: &str = "autosave";

/// A save state as advertised by the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateEntry {
    /// The name to address it by, in [`SaveSlot::State`].
    pub name: String,
    /// When it was last written.
    pub modified: SystemTime,
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
    /// Read a save, returning `Ok(None)` when none exists yet.
    fn read_save(&self, id: &SaveId, slot: &SaveSlot) -> Result<Option<Vec<u8>>, StorageError>;
    fn write_save(&self, id: &SaveId, slot: &SaveSlot, data: &[u8]) -> Result<(), StorageError>;
    /// Every save state belonging to `id`, in no particular order.
    fn list_states(&self, id: &SaveId) -> Result<Vec<StateEntry>, StorageError>;
}

/// Convenience supertrait for a backend that provides all three capabilities.
pub trait PersistentStore: SettingsStore + RomLibrary + SaveStore {}
