//! ROM parsing and battery-RAM persistence.
//!
//! The `gameboy` crate is deliberately free of filesystem concerns — it parses
//! bytes and exposes a RAM snapshot. This module bridges a loaded [`Cartridge`]
//! to the abstract [`SaveStore`]: it derives the cartridge's save identity and
//! moves battery RAM in and out of the store. All actual IO lives behind the
//! store; nothing here touches the filesystem.

use anyhow::Context;
use common::{SaveId, SaveSlot, SaveStore};
use gameboy::Cartridge;

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
        .read_save(id, SaveSlot::Battery)
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

/// Persist battery-backed cartridge RAM (and RTC, for MBC3+timer carts) through
/// the store. A no-op for carts without a battery; failures are logged rather
/// than propagated so a shutdown path can call this without a `Result`.
pub fn save_battery(cart: &Cartridge, id: &SaveId, store: &dyn SaveStore) {
    if !cart.has_battery() {
        return;
    }
    let data = cart.save_bytes();
    match store.write_save(id, SaveSlot::Battery, &data) {
        Ok(()) => tracing::info!(id = %id.0, bytes = data.len(), "wrote battery save"),
        Err(err) => tracing::error!(%err, id = %id.0, "failed to write battery save"),
    }
}
