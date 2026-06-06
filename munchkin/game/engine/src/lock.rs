//! Single-instance lock for the engine.
//!
//! The engine must guarantee that only one copy of itself runs at a time. We do
//! this with an advisory exclusive lock on a lock file (path comes from the
//! config). The lock is held by an open file descriptor, so the OS releases it
//! automatically when the process exits — even on a crash — and a second engine
//! that tries to start while one is running fails fast.

use std::fs::OpenOptions;
use std::path::Path;

use anyhow::{Context, Result, bail};
use fs4::fs_std::FileExt;

/// An acquired single-instance lock. Keep it alive for the whole process; the
/// lock is released when this value (and its file descriptor) is dropped.
pub struct InstanceLock {
    _file: std::fs::File,
}

/// Try to acquire the single-instance lock at `path`.
///
/// Returns an error (without blocking) if another engine already holds it.
pub fn acquire(path: &Path) -> Result<InstanceLock> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("opening lock file {}", path.display()))?;

    match FileExt::try_lock_exclusive(&file) {
        Ok(true) => Ok(InstanceLock { _file: file }),
        Ok(false) => bail!(
            "another engine instance is already running (lock held on {})",
            path.display()
        ),
        Err(e) => Err(e).with_context(|| format!("locking {}", path.display())),
    }
}
