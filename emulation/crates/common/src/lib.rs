//! Utilities shared across every crate in the workspace.

pub mod input;
pub mod logging;
pub mod quicksave;
pub mod scale;
pub mod settings;
pub mod storage;

pub use input::{
    Action, ActionBindings, GamepadButton, GenericAction, InputTrigger, Key, MouseButton,
};
pub use logging::{init, LogInitError};
pub use scale::{ScaleMode, Transform};
pub use settings::{
    AudioSettings, Corner, EmulationSettings, GraphicsSettings, HudSettings, InputBindings,
    SaveStateSettings, Settings, SpeedFade, SpeedMode,
};
pub use storage::{
    PersistentStore, RomEntry, RomId, RomLibrary, SaveId, SaveSlot, SaveStore, SettingsStore,
    StateEntry, StorageError,
};
