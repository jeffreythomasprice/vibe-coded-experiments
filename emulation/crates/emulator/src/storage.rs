//! File-system implementation of the [`common::storage`] traits.
//!
//! This is where every filesystem concern the abstract interface refuses to name
//! actually lives: resolving `~/.config/emulator`, the on-disk TOML schema (which
//! — unlike the backend-agnostic [`common::Settings`] — *does* carry the
//! configurable ROM/save directories), all `std::fs` IO, and turning opaque
//! identities into safe file names.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use common::input::{ActionBindings, GenericAction};
use common::{
    AudioSettings, EmulationSettings, GraphicsSettings, InputBindings, PersistentStore, RomEntry,
    RomId, RomLibrary, SaveId, SaveSlot, SaveStore, Settings, SettingsStore, StorageError,
};
use directories::ProjectDirs;
use gameboy::GameboyButton;
use serde::{Deserialize, Serialize};

/// The on-disk `settings.toml` schema. It mirrors [`common::Settings`]' fields and
/// adds file-backend / system-specific sections that the backend-agnostic
/// settings deliberately omit: a `[paths]` section for the ROM/save directories,
/// and a `[gameboy]` section for the Game Boy's input bindings (a system-specific
/// concern, kept out of `common` for the same reason `[paths]` is).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FileConfig {
    graphics: GraphicsSettings,
    audio: AudioSettings,
    emulation: EmulationSettings,
    input: InputBindings,
    gameboy: GameboyConfig,
    paths: PathsConfig,
}

/// The `[gameboy]` section: today just its input bindings (`[gameboy.input]`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct GameboyConfig {
    input: ActionBindings,
}

/// File-backend directory configuration. `None` means "use the default relative to
/// the config directory".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PathsConfig {
    saves_dir: Option<PathBuf>,
    roms_dir: Option<PathBuf>,
}

/// A file-system-backed persistent store rooted at a config directory.
pub struct FileStore {
    config_path: PathBuf,
    saves_dir: PathBuf,
    roms_dir: PathBuf,
    file: FileConfig,
}

impl FileStore {
    /// Open (or first-time initialize) the store.
    ///
    /// With no override, the config file is `<config_dir>/settings.toml`, where
    /// `config_dir` is the platform config directory for this app (e.g.
    /// `~/.config/emulator` on Linux). A missing config file is created with
    /// defaults. The saves and ROMs directories are resolved from the file (or
    /// defaulted next to the config file) and created if absent.
    pub fn open(config_override: Option<PathBuf>) -> Result<FileStore, StorageError> {
        let config_path = match config_override {
            Some(path) => path,
            None => default_config_path()?,
        };
        let config_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let file = if config_path.exists() {
            let text =
                fs::read_to_string(&config_path).map_err(|err| io_error(&config_path, err))?;
            let file: FileConfig = toml::from_str(&text).map_err(|err| {
                StorageError::Serialization(format!("{}: {err}", config_path.display()))
            })?;
            tracing::info!(path = %config_path.display(), "loaded settings");
            file
        } else {
            let file = FileConfig::default();
            create_dir(&config_dir)?;
            write_config(&config_path, &file)?;
            tracing::info!(path = %config_path.display(), "wrote default settings");
            file
        };

        let saves_dir = file
            .paths
            .saves_dir
            .clone()
            .unwrap_or_else(|| config_dir.join("saves"));
        let roms_dir = file
            .paths
            .roms_dir
            .clone()
            .unwrap_or_else(|| config_dir.join("roms"));
        create_dir(&saves_dir)?;
        create_dir(&roms_dir)?;
        tracing::debug!(
            saves = %saves_dir.display(),
            roms = %roms_dir.display(),
            "resolved storage directories"
        );

        Ok(FileStore {
            config_path,
            saves_dir,
            roms_dir,
            file,
        })
    }

    /// The currently loaded settings (the backend-agnostic subset).
    pub fn settings(&self) -> Settings {
        Settings {
            graphics: self.file.graphics.clone(),
            audio: self.file.audio.clone(),
            emulation: self.file.emulation.clone(),
            input: self.file.input.clone(),
        }
    }

    /// The loaded Game Boy input bindings (a system-specific concern that lives
    /// in the backend rather than in [`common::Settings`]).
    pub fn gameboy_bindings(&self) -> ActionBindings {
        self.file.gameboy.input.clone()
    }

    /// A complete `settings.toml` with every binding written out explicitly at
    /// its code default — the reference document emitted by `print-config`. It
    /// does not read or write any file, so it works with no config present.
    pub fn default_config_toml() -> Result<String, StorageError> {
        let config = FileConfig {
            graphics: GraphicsSettings::default(),
            audio: AudioSettings::default(),
            emulation: EmulationSettings::default(),
            input: InputBindings(ActionBindings::with_all_defaults::<GenericAction>()),
            gameboy: GameboyConfig {
                input: ActionBindings::with_all_defaults::<GameboyButton>(),
            },
            paths: PathsConfig::default(),
        };
        toml::to_string_pretty(&config).map_err(|err| StorageError::Serialization(err.to_string()))
    }

    /// Persist both the generic and Game Boy input bindings to `settings.toml`.
    /// Mirrors [`SettingsStore::save_settings`]' read-modify-write so the other
    /// sections (notably a hand-edited `[paths]`) survive the rewrite. Game Boy
    /// bindings live outside [`common::Settings`], so this is a `FileStore`
    /// method rather than part of the `SettingsStore` trait — it is the write
    /// path for the in-app bindings editor.
    pub fn save_input_bindings(
        &self,
        generic: &InputBindings,
        gameboy: &ActionBindings,
    ) -> Result<(), StorageError> {
        let mut file: FileConfig = match fs::read_to_string(&self.config_path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|_| self.file.clone()),
            Err(_) => self.file.clone(),
        };
        file.input = generic.clone();
        file.gameboy.input = gameboy.clone();
        write_config(&self.config_path, &file)?;
        tracing::info!(path = %self.config_path.display(), "saved input bindings");
        Ok(())
    }

    /// Path a `(SaveId, SaveSlot)` maps to under the saves directory.
    fn save_path(&self, id: &SaveId, slot: SaveSlot) -> PathBuf {
        let name = sanitize(&id.0);
        let file = match slot {
            SaveSlot::Battery => format!("{name}.sav"),
            SaveSlot::State(n) => format!("{name}.state{n}"),
        };
        self.saves_dir.join(file)
    }
}

impl SettingsStore for FileStore {
    fn load_settings(&self) -> Result<Settings, StorageError> {
        Ok(self.settings())
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), StorageError> {
        // Re-read so a hand-edited `[paths]` section is preserved across the write.
        let mut file = match fs::read_to_string(&self.config_path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|_| self.file.clone()),
            Err(_) => self.file.clone(),
        };
        file.graphics = settings.graphics.clone();
        file.emulation = settings.emulation.clone();
        file.input = settings.input.clone();
        write_config(&self.config_path, &file)?;
        tracing::info!(path = %self.config_path.display(), "saved settings");
        Ok(())
    }
}

impl RomLibrary for FileStore {
    fn list_roms(&self) -> Result<Vec<RomEntry>, StorageError> {
        let mut roms = Vec::new();
        collect_roms(&self.roms_dir, &self.roms_dir, &mut roms)?;
        roms.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        tracing::debug!(count = roms.len(), "listed roms");
        Ok(roms)
    }

    fn read_rom(&self, id: &RomId) -> Result<Vec<u8>, StorageError> {
        let path = self.roms_dir.join(&id.0);
        tracing::debug!(path = %path.display(), "reading rom from library");
        fs::read(&path).map_err(|err| io_error(&path, err))
    }
}

impl SaveStore for FileStore {
    fn read_save(&self, id: &SaveId, slot: SaveSlot) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.save_path(id, slot);
        match fs::read(&path) {
            Ok(data) => {
                tracing::info!(path = %path.display(), bytes = data.len(), "read save");
                Ok(Some(data))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                tracing::debug!(path = %path.display(), "no save yet");
                Ok(None)
            }
            Err(err) => Err(io_error(&path, err)),
        }
    }

    fn write_save(&self, id: &SaveId, slot: SaveSlot, data: &[u8]) -> Result<(), StorageError> {
        let path = self.save_path(id, slot);
        fs::write(&path, data).map_err(|err| io_error(&path, err))?;
        tracing::info!(path = %path.display(), bytes = data.len(), "wrote save");
        Ok(())
    }
}

impl PersistentStore for FileStore {}

/// Recursively collect every regular file under `dir` as a [`RomEntry`]. ROMs
/// nested in sub-directories count, so the [`RomId`] is the path **relative to
/// `root`** (the configured roms dir) — that is exactly what [`FileStore::read_rom`]
/// re-joins onto `roms_dir` to read the bytes back. The display `name` is the file
/// stem. Symlink loops are not guarded against.
fn collect_roms(root: &Path, dir: &Path, out: &mut Vec<RomEntry>) -> Result<(), StorageError> {
    let entries = fs::read_dir(dir).map_err(|err| io_error(dir, err))?;
    for entry in entries {
        let entry = entry.map_err(|err| io_error(dir, err))?;
        let path = entry.path();
        if path.is_dir() {
            collect_roms(root, &path, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(rel) = path.strip_prefix(root).ok().and_then(|p| p.to_str()) else {
            continue;
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(rel)
            .to_string();
        out.push(RomEntry {
            id: RomId(rel.to_string()),
            name,
        });
    }
    Ok(())
}

/// `<config_dir>/settings.toml` for this application on this platform.
fn default_config_path() -> Result<PathBuf, StorageError> {
    let dirs = ProjectDirs::from("", "", "emulator")
        .ok_or_else(|| StorageError::Config("could not determine a config directory".into()))?;
    Ok(dirs.config_dir().join("settings.toml"))
}

fn create_dir(path: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(path).map_err(|err| io_error(path, err))
}

fn write_config(path: &Path, config: &FileConfig) -> Result<(), StorageError> {
    let text = toml::to_string_pretty(config)
        .map_err(|err| StorageError::Serialization(err.to_string()))?;
    fs::write(path, text).map_err(|err| io_error(path, err))
}

fn io_error(path: &Path, err: io::Error) -> StorageError {
    StorageError::Io(format!("{}: {err}", path.display()))
}

/// Reduce an opaque identity to a filesystem-safe base name. Keeps ASCII
/// alphanumerics, `_`, and `-`; everything else (including path separators and
/// `..`) becomes `_`. Guards against an empty result.
fn sanitize(id: &str) -> String {
    let mut out: String = id
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("save");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use common::{RomId, ScaleMode};

    use super::*;

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize("TETRIS"), "TETRIS");
        assert_eq!(sanitize("Zelda-DX_1"), "Zelda-DX_1");
    }

    #[test]
    fn sanitize_replaces_unsafe_chars_and_traversal() {
        assert_eq!(sanitize("../etc/passwd"), "___etc_passwd");
        assert_eq!(sanitize("a b/c"), "a_b_c");
    }

    #[test]
    fn sanitize_falls_back_when_empty() {
        assert_eq!(sanitize("   "), "save");
    }

    /// A unique scratch config path per test invocation.
    fn scratch_config() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("emulator-test-{}-{n}", std::process::id()))
            .join("settings.toml")
    }

    #[test]
    fn open_writes_defaults_and_creates_dirs() {
        let config = scratch_config();
        let store = FileStore::open(Some(config.clone())).unwrap();
        assert!(config.exists(), "settings.toml should be written");
        assert!(store.saves_dir.is_dir(), "saves dir should be created");
        assert!(store.roms_dir.is_dir(), "roms dir should be created");
        assert_eq!(store.settings().graphics.scale_mode, ScaleMode::Fit);
        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn settings_survive_reopen_and_preserve_paths() {
        let config = scratch_config();
        let custom_roms = config.parent().unwrap().join("custom-roms");
        // Seed with a custom [paths] the store should preserve across a settings write.
        create_dir(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            format!(
                "[paths]\nroms_dir = \"{}\"\n[graphics]\nscale_mode = \"original\"\n",
                custom_roms.display()
            ),
        )
        .unwrap();

        let store = FileStore::open(Some(config.clone())).unwrap();
        assert_eq!(store.settings().graphics.scale_mode, ScaleMode::Original);
        assert_eq!(store.roms_dir, custom_roms);

        // Change a setting and persist it.
        let mut settings = store.settings();
        settings.graphics.scale_mode = ScaleMode::Stretch;
        settings.emulation.speed = common::SpeedMode::Relative(2.0);
        store.save_settings(&settings).unwrap();

        // Reopen (simulating a relaunch): the new values load and [paths] is intact.
        let reopened = FileStore::open(Some(config.clone())).unwrap();
        assert_eq!(reopened.settings().graphics.scale_mode, ScaleMode::Stretch);
        assert_eq!(
            reopened.settings().emulation.speed,
            common::SpeedMode::Relative(2.0)
        );
        assert_eq!(reopened.roms_dir, custom_roms);

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn battery_save_round_trips_across_reopen() {
        let config = scratch_config();
        let id = SaveId("Test Game".to_string());

        {
            let store = FileStore::open(Some(config.clone())).unwrap();
            assert!(store.read_save(&id, SaveSlot::Battery).unwrap().is_none());
            store
                .write_save(&id, SaveSlot::Battery, b"savedata")
                .unwrap();
            // The sanitized name (space -> underscore) is what hits disk.
            assert!(store.saves_dir.join("Test_Game.sav").is_file());
        }

        // Reopen and confirm the save is found.
        let store = FileStore::open(Some(config.clone())).unwrap();
        let data = store.read_save(&id, SaveSlot::Battery).unwrap();
        assert_eq!(data.as_deref(), Some(&b"savedata"[..]));

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn default_config_toml_round_trips_and_fills_bindings() {
        use common::input::{InputTrigger, Key};

        let text = FileStore::default_config_toml().unwrap();
        // Both sections are present with explicit bindings (rendered as TOML
        // arrays-of-tables, e.g. `[[input.menu]]` / `[[gameboy.input.up]]`).
        assert!(text.contains("input.menu"), "generic input section: {text}");
        assert!(text.contains("gameboy.input"), "gameboy section: {text}");

        let config: FileConfig = toml::from_str(&text).unwrap();
        assert_eq!(
            config.input.0.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Escape)]
        );
        assert_eq!(
            config.gameboy.input.triggers(&GameboyButton::A),
            vec![InputTrigger::Key(Key::KeyX)]
        );
    }

    #[test]
    fn gameboy_input_section_loads() {
        use common::input::{InputTrigger, Key};

        let config = scratch_config();
        create_dir(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            "[gameboy.input]\nup = [{ key = \"key_w\" }]\ndown = []\n",
        )
        .unwrap();

        let store = FileStore::open(Some(config.clone())).unwrap();
        let bindings = store.gameboy_bindings();
        // Explicit rebinding is honored.
        assert_eq!(
            bindings.triggers(&GameboyButton::Up),
            vec![InputTrigger::Key(Key::KeyW)]
        );
        // Empty array means unbound.
        assert!(bindings.triggers(&GameboyButton::Down).is_empty());
        // Absent action falls back to its default.
        assert_eq!(
            bindings.triggers(&GameboyButton::A),
            vec![InputTrigger::Key(Key::KeyX)]
        );

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn save_input_bindings_round_trips_both_sections_and_preserves_paths() {
        use common::input::{InputTrigger, Key};

        let config = scratch_config();
        let custom_roms = config.parent().unwrap().join("custom-roms");
        create_dir(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            format!("[paths]\nroms_dir = \"{}\"\n", custom_roms.display()),
        )
        .unwrap();

        let store = FileStore::open(Some(config.clone())).unwrap();

        // Rebind a generic action and a Game Boy button, then persist.
        let mut generic = store.settings().input;
        generic.0.set("menu", vec![InputTrigger::Key(Key::Tab)]);
        let mut gameboy = store.gameboy_bindings();
        gameboy.set("a", vec![InputTrigger::Key(Key::KeyQ)]);
        store.save_input_bindings(&generic, &gameboy).unwrap();

        // Reopen: both edits load and the hand-edited [paths] survives.
        let reopened = FileStore::open(Some(config.clone())).unwrap();
        assert_eq!(
            reopened.settings().input.0.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Tab)]
        );
        assert_eq!(
            reopened.gameboy_bindings().triggers(&GameboyButton::A),
            vec![InputTrigger::Key(Key::KeyQ)]
        );
        assert_eq!(reopened.roms_dir, custom_roms);

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn list_roms_reports_dropped_files() {
        let config = scratch_config();
        let store = FileStore::open(Some(config.clone())).unwrap();
        fs::write(store.roms_dir.join("game.gb"), b"\0").unwrap();

        let roms = store.list_roms().unwrap();
        assert_eq!(roms.len(), 1);
        assert_eq!(roms[0].id, RomId("game.gb".to_string()));
        assert_eq!(roms[0].name, "game");
        assert_eq!(store.read_rom(&roms[0].id).unwrap(), b"\0");

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn list_roms_scans_subdirectories() {
        let config = scratch_config();
        let store = FileStore::open(Some(config.clone())).unwrap();
        let nested = store.roms_dir.join("gb").join("puzzle");
        create_dir(&nested).unwrap();
        fs::write(store.roms_dir.join("top.gb"), b"t").unwrap();
        fs::write(nested.join("deep.gb"), b"d").unwrap();

        let roms = store.list_roms().unwrap();
        assert_eq!(roms.len(), 2);
        // The id is the path relative to the roms dir, so read_rom re-joins it.
        let deep = roms
            .iter()
            .find(|r| r.name == "deep")
            .expect("nested rom listed");
        assert_eq!(deep.id, RomId("gb/puzzle/deep.gb".to_string()));
        assert_eq!(store.read_rom(&deep.id).unwrap(), b"d");

        let _ = fs::remove_dir_all(config.parent().unwrap());
    }
}
