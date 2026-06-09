use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use fs2::FileExt;

/// An advisory exclusive lock on a file, held for the lifetime of this value.
///
/// The lock is an OS-level `flock`, so it is released automatically when the
/// process exits — even on a crash or `SIGKILL`. That means a leftover lock file
/// on disk is harmless: the next instance can always re-acquire it.
pub struct InstanceLock {
    // Held only to keep the lock alive; never read after construction.
    _file: File,
}

impl InstanceLock {
    /// Try to acquire the single-instance lock at `path`.
    ///
    /// - `Ok(Some(lock))` — acquired; hold it for the process lifetime.
    /// - `Ok(None)` — another instance already holds the lock.
    /// - `Err(_)` — the lock file couldn't be opened.
    pub fn acquire(path: &Path) -> std::io::Result<Option<InstanceLock>> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                // Record our pid for diagnostics; best-effort.
                let _ = file.set_len(0);
                let _ = write!(file, "{}", std::process::id());
                let _ = file.flush();
                Ok(Some(InstanceLock { _file: file }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
