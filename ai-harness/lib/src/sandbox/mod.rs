//! Confining a subprocess to a project's [`crate::vfs::MountTable`].
//!
//! The sandbox's filesystem is **a read-only system image ∪ the project's
//! mount table**, not the mount table alone: a shell needs `ld.so`, libc,
//! and a handful of binaries to exec at all, so `[sandbox] system_paths`
//! (`/usr`, `/bin`, minimal `/etc`, …) is bound read-only underneath every
//! sandbox. `lib::vfs::Vfs` — the in-process file primitives — sees only the
//! project's own mounts, never `system_paths`. A tool that reads files
//! answers "what is in my project"; a shell needs "what is in my project,
//! plus enough OS to run." They are deliberately different, and
//! `system_paths` is the knob that controls the difference. Both consume
//! the *same* [`crate::vfs::MountTable`] for the project's own directories,
//! so they can never disagree about those.
//!
//! [`bwrap::bwrap_argv`] is the pure, unit-tested core: a [`SandboxSpec`] in,
//! an argv out, no process ever spawned. Every empirical claim below was
//! checked against a real, unprivileged `bwrap` run on this machine (Ubuntu
//! 24.04, kernel 7.0, Landlock ABI 8) before being encoded here:
//!
//! - `--unshare-all` plus a per-mount `--ro-bind`/`--bind` reproduces the
//!   virtual filesystem exactly: a scaffold directory lists only its
//!   mounted children, an unmounted directory doesn't exist, and an empty
//!   mounted directory is preserved.
//! - a symlink inside a mount pointing outside it is **structurally dead**
//!   under the namespace — the target simply isn't there — which is why
//!   `lib::vfs::ops` deliberately mirrors this with `openat2`'s
//!   `RESOLVE_IN_ROOT` instead of `canonicalize` + a prefix check.
//! - mounts **must** be bound parents-before-children — binding a read-only
//!   parent *after* an already-bound read-write child silently clobbers the
//!   child back to read-only, with no error. [`crate::vfs::MountTable`]'s
//!   sort order exists specifically to guarantee this.
//! - a scaffold directory is writable *inside* the sandbox (ephemeral
//!   tmpfs) but nothing written there is ever visible on the host once the
//!   sandbox exits — the one place `lib::vfs::Vfs` (which refuses the write
//!   outright) and this layer diverge on whether an operation succeeds,
//!   though never on whether anything actually escapes.
//! - `--new-session` matters: without it a sandboxed process can push
//!   characters into the controlling terminal via `TIOCSTI`.
//! - `bwrap --version` succeeding is **not** proof the sandbox works —
//!   Ubuntu 24.04's `apparmor_restrict_unprivileged_userns` can let the
//!   binary run while `--unshare-user` still fails at exec time. Backend
//!   detection here always does a real trivial exec — see [`bwrap::probe`].
//!
//! This module builds and (optionally) runs the confined command; it does
//! not decide what to run. That's deliberately out of scope here — the
//! filesystem/bash tools that call this are a later addition, on top of the
//! mechanism this module and `lib::vfs` provide.

pub mod bwrap;
pub mod error;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

pub use bwrap::Bubblewrap;
pub use error::SandboxError;

use crate::config::SandboxConfig;
use crate::vfs::MountTable;

/// What to run, and the project mount table to confine it to.
pub struct SandboxSpec {
    pub table: MountTable,
    pub program: OsString,
    pub args: Vec<OsString>,
    /// Must resolve `Inside` `table`, or a mount's root when `None`. Backends
    /// don't validate this themselves — `lib::service` does, since it's the
    /// layer that already has a `Vfs` to ask.
    pub cwd: Option<PathBuf>,
    pub network: bool,
}

/// Whether a sandbox backend actually works on this machine, as determined
/// by a real trivial exec — see [`bwrap::probe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable { reason: String },
}

impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available)
    }
}

/// A way to run a confined command. `command()` is pure — it builds a
/// [`std::process::Command`] and spawns nothing — so the confinement policy
/// itself (the exact argv) is what a unit test asserts on, the same way
/// `lib::llm` tests request building as a pure function over strings.
pub trait SandboxBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn command(&self, spec: &SandboxSpec) -> Result<std::process::Command, SandboxError>;
}

/// The backend selected when sandboxing is disabled in config, or the real
/// backend failed its startup probe. Every call is a hard, typed error —
/// **never** a silent fall-through to running unconfined.
pub struct Disabled {
    reason: String,
}

impl Disabled {
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}

impl SandboxBackend for Disabled {
    fn name(&self) -> &'static str {
        "none"
    }

    fn command(&self, _spec: &SandboxSpec) -> Result<std::process::Command, SandboxError> {
        Err(SandboxError::Unavailable {
            reason: self.reason.clone(),
        })
    }
}

/// Resolve the configured backend and probe it. Call once at startup — the
/// probe execs a real (harmless) sandboxed process, so it isn't free.
pub fn detect(config: &SandboxConfig) -> (Arc<dyn SandboxBackend>, Availability) {
    if !config.enabled {
        let reason = "sandboxing disabled in config (`[sandbox] enabled = false`)".to_string();
        return (
            Arc::new(Disabled { reason: reason.clone() }),
            Availability::Unavailable { reason },
        );
    }
    let availability = bwrap::probe(&config.bwrap_path, &config.system_paths);
    let backend: Arc<dyn SandboxBackend> = match &availability {
        Availability::Available => Arc::new(Bubblewrap::new(config.bwrap_path.clone(), config.system_paths.clone())),
        Availability::Unavailable { reason } => Arc::new(Disabled { reason: reason.clone() }),
    };
    (backend, availability)
}
