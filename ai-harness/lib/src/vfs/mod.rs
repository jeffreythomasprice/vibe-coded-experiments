//! A project's virtual filesystem: a filter over the real one, not a
//! relocation. Given real paths `/a/b/c.txt`, `/d`, `/e/f.txt` and a project
//! that grants `/a/b` and `/d`, the virtual filesystem contains exactly
//! `/a/b/c.txt` and `/d` (still an empty directory) — `/a` exists only as a
//! synthetic scaffold listing `b`, and `/e` does not exist at all. Paths keep
//! their real absolute form on both sides; nothing is remapped.
//!
//! [`mount::MountTable`] is the pure, filesystem-free core: normalizing a
//! project's directory list and answering "what does this path resolve to."
//! [`ops::Vfs`] is the filesystem-backed surface built on top of it.
//! `lib::sandbox::bwrap_argv` consumes the *same* `MountTable` to build a
//! subprocess's mount namespace, so an in-process file tool and a shell
//! running inside the sandbox can never disagree about what exists — see
//! that module's doc for the one place they deliberately do (the read-only
//! system layer a shell needs to have an OS to run in at all).
//!
//! `openat2` with `RESOLVE_IN_ROOT`/`RESOLVE_NO_MAGICLINKS` is Linux 5.6+.
//! Consistent with the rest of this workspace being Linux-only in fact
//! (every config default is a POSIX path; `server/gen/schemas/` ships only a
//! Linux schema), that requirement is stated here rather than papered over
//! with a fallback for a platform this application does not run on.

pub mod error;
pub mod mount;
pub mod ops;

pub use error::VfsError;
pub use mount::{normalize, Mount, MountTable, Resolution};
pub use ops::{Entry, Vfs, VfsMetadata};
