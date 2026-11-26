//! ROM identity, battery-RAM persistence, and save states.
//!
//! The `gameboy` crate is deliberately free of filesystem concerns — it parses
//! bytes and exposes a RAM snapshot. This module bridges a loaded [`Cartridge`]
//! to the abstract [`SaveStore`]: it derives the cartridge's save identity and
//! moves battery RAM and save states in and out of the store. All IO lives behind the
//! store; nothing here touches the filesystem.

use anyhow::Context;
use common::{RomId, RomLibrary, SaveId, SaveSlot, SaveStore, StateEntry};
use gameboy::{Cartridge, Emulator};

/// The save identity for a cartridge: its (trimmed) header title, falling back to
/// `source_name` when the title is blank (common for homebrew and test ROMs). The
/// store is responsible for turning this into a safe file name.
pub fn save_id_for(cart: &Cartridge, source_name: &str) -> SaveId {
    let title = cart.header().title.trim();
    if title.is_empty() {
        SaveId(source_name.to_string())
    } else {
        SaveId(title.to_string())
    }
}

/// Restore battery-backed RAM for a freshly parsed cartridge, if any exists.
pub fn restore_battery(
    cart: &mut Cartridge,
    id: &SaveId,
    store: &dyn SaveStore,
) -> anyhow::Result<()> {
    if !cart.has_battery() {
        return Ok(());
    }
    match store
        .read_save(id, &SaveSlot::Battery)
        .with_context(|| format!("failed to read battery save for {}", id.0))?
    {
        Some(data) => {
            cart.load_bytes(&data);
            tracing::info!(id = %id.0, bytes = data.len(), "restored battery save");
        }
        None => tracing::debug!(id = %id.0, "no battery save to restore"),
    }
    Ok(())
}

/// Read and parse a ROM from the library and restore its battery save, returning
/// the cartridge with its save identity.
pub fn open_rom<S: RomLibrary + SaveStore>(
    id: &RomId,
    source_name: &str,
    store: &S,
) -> anyhow::Result<(Cartridge, SaveId)> {
    let bytes = store
        .read_rom(id)
        .with_context(|| format!("failed to read rom {}", id.0))?;
    let mut cart = Cartridge::from_bytes(&bytes)
        .with_context(|| format!("failed to parse rom {}", id.0))?;
    let save_id = save_id_for(&cart, source_name);
    restore_battery(&mut cart, &save_id, store)?;
    Ok((cart, save_id))
}

/// Persist battery-backed cartridge RAM (and RTC, for MBC3+timer carts) through
/// the store. A no-op for carts without a battery; failures are logged rather
/// than propagated so a shutdown path can call this without a `Result`.
pub fn save_battery(cart: &Cartridge, id: &SaveId, store: &dyn SaveStore) {
    if !cart.has_battery() {
        return;
    }
    let data = cart.save_bytes();
    match store.write_save(id, &SaveSlot::Battery, &data) {
        Ok(()) => tracing::info!(id = %id.0, bytes = data.len(), "wrote battery save"),
        Err(err) => tracing::error!(%err, id = %id.0, "failed to write battery save"),
    }
}

/// The cartridge's save states, newest first.
pub fn list_states(id: &SaveId, store: &dyn SaveStore) -> anyhow::Result<Vec<StateEntry>> {
    let mut states = store
        .list_states(id)
        .with_context(|| format!("failed to list save states for {}", id.0))?;
    states.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    Ok(states)
}

/// Snapshot the machine into the save state called `name`, replacing any
/// existing one.
pub fn save_state(
    emu: &Emulator,
    id: &SaveId,
    name: &str,
    store: &dyn SaveStore,
) -> anyhow::Result<()> {
    let data = emu.save_state().context("failed to capture the machine state")?;
    store
        .write_save(id, &SaveSlot::State(name.to_string()), &data)
        .with_context(|| format!("failed to write save state {name:?}"))?;
    tracing::info!(id = %id.0, name, bytes = data.len(), "saved state");
    Ok(())
}

/// Restore the machine from the save state called `name`. On error the machine
/// is left as it was.
pub fn load_state(
    emu: &mut Emulator,
    id: &SaveId,
    name: &str,
    store: &dyn SaveStore,
) -> anyhow::Result<()> {
    let data = store
        .read_save(id, &SaveSlot::State(name.to_string()))
        .with_context(|| format!("failed to read save state {name:?}"))?
        .with_context(|| format!("there is no save state {name:?}"))?;
    emu.load_state(&data)
        .with_context(|| format!("failed to load save state {name:?}"))?;
    tracing::info!(id = %id.0, name, "loaded state");
    Ok(())
}

/// What [`resume_latest`] managed to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Resume {
    /// The machine was restored from the state of this name.
    Loaded(String),
    /// There was nothing to resume.
    NoStates,
    /// There are saves, but none could be loaded (or they couldn't be listed).
    Failed,
}

/// Restore the machine from the most recent save state that will load, whatever
/// kind it is. A state that fails is skipped in favor of the next newest.
pub fn resume_latest(emu: &mut Emulator, id: &SaveId, store: &dyn SaveStore) -> Resume {
    let states = match list_states(id, store) {
        Ok(states) => states,
        Err(err) => {
            tracing::error!("{err:#}");
            return Resume::Failed;
        }
    };
    if states.is_empty() {
        return Resume::NoStates;
    }
    for state in states {
        match load_state(emu, id, &state.name, store) {
            Ok(()) => return Resume::Loaded(state.name),
            Err(err) => tracing::warn!("skipping a save state: {err:#}"),
        }
    }
    Resume::Failed
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::{Duration, UNIX_EPOCH};

    use common::StorageError;

    use super::*;

    /// Save states held in memory, with chosen dates.
    #[derive(Default)]
    struct MemoryStore(RefCell<Vec<(StateEntry, Vec<u8>)>>);

    impl MemoryStore {
        fn add(&self, name: &str, secs: u64, data: Vec<u8>) {
            let entry = StateEntry {
                name: name.to_string(),
                modified: UNIX_EPOCH + Duration::from_secs(secs),
            };
            self.0.borrow_mut().push((entry, data));
        }
    }

    impl SaveStore for MemoryStore {
        fn read_save(&self, _: &SaveId, slot: &SaveSlot) -> Result<Option<Vec<u8>>, StorageError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|(entry, _)| *slot == SaveSlot::State(entry.name.clone()))
                .map(|(_, data)| data.clone()))
        }

        fn write_save(&self, _: &SaveId, _: &SaveSlot, _: &[u8]) -> Result<(), StorageError> {
            Err(StorageError::Io("read-only".to_string()))
        }

        fn list_states(&self, _: &SaveId) -> Result<Vec<StateEntry>, StorageError> {
            Ok(self.0.borrow().iter().map(|(e, _)| e.clone()).collect())
        }
    }

    fn machine() -> Emulator {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]); // spin
        Emulator::new(Cartridge::from_bytes(&rom).unwrap())
    }

    /// A state taken after running `frames` frames.
    fn state_after(frames: usize) -> Vec<u8> {
        let mut emu = machine();
        for _ in 0..frames {
            emu.run_frame();
        }
        emu.save_state().unwrap()
    }

    fn resume(store: &MemoryStore) -> (Resume, Vec<u8>) {
        let mut emu = machine();
        let outcome = resume_latest(&mut emu, &SaveId("game".to_string()), store);
        (outcome, emu.save_state().unwrap())
    }

    #[test]
    fn the_newest_state_wins_whatever_its_name() {
        let store = MemoryStore::default();
        store.add("quicksave-1", 10, state_after(1));
        store.add("boss", 30, state_after(3));
        store.add("autosave", 20, state_after(2));

        let (outcome, machine_state) = resume(&store);
        assert_eq!(outcome, Resume::Loaded("boss".to_string()));
        assert_eq!(machine_state, state_after(3));
    }

    #[test]
    fn a_state_that_will_not_load_is_skipped() {
        let store = MemoryStore::default();
        store.add("quicksave-1", 10, state_after(1));
        store.add("autosave", 20, b"damaged".to_vec());

        let (outcome, machine_state) = resume(&store);
        assert_eq!(outcome, Resume::Loaded("quicksave-1".to_string()));
        assert_eq!(machine_state, state_after(1));
    }

    #[test]
    fn nothing_loadable_leaves_the_machine_at_power_on() {
        let store = MemoryStore::default();
        assert_eq!(resume(&store), (Resume::NoStates, state_after(0)));

        store.add("autosave", 20, b"damaged".to_vec());
        assert_eq!(resume(&store), (Resume::Failed, state_after(0)));
    }
}
