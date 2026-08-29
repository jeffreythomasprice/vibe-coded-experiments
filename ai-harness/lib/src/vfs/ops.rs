//! In-process file primitives over a [`MountTable`].
//!
//! Containment uses `openat2(RESOLVE_IN_ROOT | RESOLVE_NO_MAGICLINKS)`
//! against a freshly opened directory fd for the target mount, not
//! `canonicalize` + a `starts_with` check. Two reasons:
//!
//! - **It agrees with `lib::sandbox`.** Under `RESOLVE_IN_ROOT`, a symlink
//!   inside a mount that points outside it (say `/a/b/escape -> /e`)
//!   resolves *relative to the mount root* — so `/e` inside the mount is
//!   just another path under the mount, almost certainly absent, and the
//!   lookup fails exactly the way it fails inside a real `bwrap` sandbox
//!   (verified empirically — see `lib::sandbox`'s module doc). A
//!   canonicalize-based check would instead resolve the symlink to the real
//!   `/e/f.txt` and then have to notice, after the fact, that it escaped.
//!   Two enforcement layers computing different answers for the same
//!   project is a bug generator; this makes them compute the same answer by
//!   construction.
//! - **It is race-free.** `canonicalize`-then-open has a TOCTOU window a
//!   symlink swap can drive through; resolving and opening happen in one
//!   syscall.
//!
//! Accepted residual risk, written down rather than hidden: this opens a
//! **fresh** directory fd for the mount root on every call rather than
//! holding one open for the table's lifetime, so a mount root itself being
//! replaced between calls is not defended against. That's a much narrower
//! window than the symlink-escape case above, and the mount roots are
//! server-side configuration, not attacker input.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{self as rfs, Mode, OFlags, ResolveFlags};

use shared::project::AccessMode;

use super::error::VfsError;
use super::mount::{normalize, MountTable, Resolution};

/// One entry in a [`Vfs::read_dir`] listing. Carries only a name and a kind —
/// no size or timestamps — since nothing in this workspace consumes more
/// than that yet; a tool that needs `metadata` calls [`Vfs::metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Dir(String),
    File(String),
    Symlink(String),
    Other(String),
}

impl Entry {
    pub fn name(&self) -> &str {
        match self {
            Entry::Dir(name) | Entry::File(name) | Entry::Symlink(name) | Entry::Other(name) => name,
        }
    }
}

/// A file's kind and size, resolved through the same containment path as
/// every other operation here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsMetadata {
    pub is_dir: bool,
    pub is_file: bool,
    pub len: u64,
}

/// The read/write surface of a project's virtual filesystem.
pub struct Vfs {
    table: MountTable,
}

impl Vfs {
    pub fn new(table: MountTable) -> Self {
        Self { table }
    }

    pub fn table(&self) -> &MountTable {
        &self.table
    }

    /// List a directory. A [`Resolution::Scaffold`] is synthesized entirely
    /// from the mount table and never touches the real filesystem — a
    /// sibling file that exists in the real directory but isn't itself a
    /// project directory stays invisible.
    pub fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, VfsError> {
        let normalized = normalize(path)?;
        match self.table.resolve(&normalized) {
            Resolution::Scaffold { children } => Ok(children.into_iter().map(Entry::Dir).collect()),
            Resolution::Outside => Err(VfsError::Outside { path: normalized }),
            Resolution::Inside { .. } => {
                let dir_fd = self.open_contained(&normalized, OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty())?;
                let dir = rfs::Dir::read_from(&dir_fd).map_err(|source| self.io_err(&normalized, source))?;
                let mut entries = Vec::new();
                for entry in dir {
                    let entry = entry.map_err(|source| self.io_err(&normalized, source))?;
                    let raw = entry.file_name().to_bytes();
                    if raw == b"." || raw == b".." {
                        continue;
                    }
                    let name = OsStr::from_bytes(raw).to_string_lossy().into_owned();
                    entries.push(match entry.file_type() {
                        rfs::FileType::Directory => Entry::Dir(name),
                        rfs::FileType::RegularFile => Entry::File(name),
                        rfs::FileType::Symlink => Entry::Symlink(name),
                        _ => Entry::Other(name),
                    });
                }
                entries.sort_by(|a, b| a.name().cmp(b.name()));
                Ok(entries)
            }
        }
    }

    /// Metadata for a path. Like [`Vfs::read_dir`], a scaffold ancestor
    /// reports as an (empty) directory without touching the filesystem.
    pub fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError> {
        let normalized = normalize(path)?;
        match self.table.resolve(&normalized) {
            Resolution::Scaffold { .. } => Ok(VfsMetadata {
                is_dir: true,
                is_file: false,
                len: 0,
            }),
            Resolution::Outside => Err(VfsError::Outside { path: normalized }),
            Resolution::Inside { .. } => {
                let fd = self.open_contained(&normalized, OFlags::RDONLY, Mode::empty())?;
                let stat = rfs::fstat(&fd).map_err(|source| self.io_err(&normalized, source))?;
                let file_type = rfs::FileType::from_raw_mode(stat.st_mode);
                Ok(VfsMetadata {
                    is_dir: file_type == rfs::FileType::Directory,
                    is_file: file_type == rfs::FileType::RegularFile,
                    len: stat.st_size as u64,
                })
            }
        }
    }

    pub fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        let normalized = normalize(path)?;
        self.require_inside(&normalized)?;
        let fd = self.open_contained(&normalized, OFlags::RDONLY, Mode::empty())?;
        let mut file = File::from(fd);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|source| VfsError::Io {
                path: normalized.clone(),
                source,
            })?;
        Ok(buf)
    }

    pub fn read_to_string(&self, path: &Path) -> Result<String, VfsError> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|_| VfsError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "not valid UTF-8"),
        })
    }

    /// Create (or truncate) and write a file. Refuses a read-only mount and
    /// refuses to write through or over an existing symlink — see the
    /// `NO_SYMLINKS` resolve flag below, which enforces the latter in the
    /// kernel rather than via a separate `symlink_metadata` check that could
    /// itself race.
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), VfsError> {
        let normalized = normalize(path)?;
        match self.table.resolve(&normalized) {
            Resolution::Inside {
                mode: AccessMode::ReadOnly,
                ..
            } => return Err(VfsError::ReadOnly { path: normalized }),
            Resolution::Inside { .. } => {}
            // A scaffold directory is synthetic and read-only by
            // construction — see this crate's `lib::sandbox` module doc for
            // the one place this diverges from `bwrap`, where a scaffold
            // directory is writable but ephemeral (tmpfs, discarded with the
            // sandbox). Nothing escapes in either case; there is simply no
            // real location for a `Vfs` write to land, so it is refused.
            Resolution::Scaffold { .. } => return Err(VfsError::ReadOnly { path: normalized }),
            Resolution::Outside => return Err(VfsError::Outside { path: normalized }),
        }
        let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC;
        let mode = Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::ROTH;
        let resolve = ResolveFlags::IN_ROOT | ResolveFlags::NO_MAGICLINKS | ResolveFlags::NO_SYMLINKS;
        let fd = self.open_with(&normalized, flags, mode, resolve).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists
                || source.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error())
            {
                VfsError::SymlinkTarget {
                    path: normalized.clone(),
                }
            } else {
                VfsError::Io {
                    path: normalized.clone(),
                    source,
                }
            }
        })?;
        File::from(fd).write_all(bytes).map_err(|source| VfsError::Io {
            path: normalized.clone(),
            source,
        })
    }

    pub fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        let normalized = normalize(path)?;
        match self.table.resolve(&normalized) {
            Resolution::Inside {
                mode: AccessMode::ReadOnly,
                ..
            } => return Err(VfsError::ReadOnly { path: normalized }),
            Resolution::Inside { .. } => {}
            Resolution::Scaffold { .. } | Resolution::Outside => {
                return Err(VfsError::Outside { path: normalized });
            }
        }
        let (root, rel) = self.split(&normalized)?;
        let mut built = root;
        for component in rel.components() {
            built.push(component);
            match std::fs::create_dir(&built) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(source) => {
                    return Err(VfsError::Io {
                        path: normalized.clone(),
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        let normalized = normalize(path)?;
        match self.table.resolve(&normalized) {
            Resolution::Inside {
                mode: AccessMode::ReadOnly,
                ..
            } => return Err(VfsError::ReadOnly { path: normalized }),
            Resolution::Inside { .. } => {}
            Resolution::Scaffold { .. } | Resolution::Outside => {
                return Err(VfsError::Outside { path: normalized });
            }
        }
        let (root, rel) = self.split(&normalized)?;
        let root_fd = open_dir_no_follow(&root).map_err(|source| VfsError::Io {
            path: normalized.clone(),
            source,
        })?;
        rfs::unlinkat(&root_fd, &rel, rfs::AtFlags::empty()).map_err(|source| VfsError::Io {
            path: normalized.clone(),
            source: source.into(),
        })
    }

    // --- helpers ---

    fn require_inside(&self, path: &Path) -> Result<(), VfsError> {
        match self.table.resolve(path) {
            Resolution::Inside { .. } => Ok(()),
            Resolution::Scaffold { .. } | Resolution::Outside => Err(VfsError::Outside { path: path.to_path_buf() }),
        }
    }

    fn split(&self, normalized: &Path) -> Result<(PathBuf, PathBuf), VfsError> {
        let Resolution::Inside { mount, .. } = self.table.resolve(normalized) else {
            return Err(VfsError::Outside {
                path: normalized.to_path_buf(),
            });
        };
        let root = self.table.mounts()[mount].root.clone();
        let rel = normalized.strip_prefix(&root).expect("mount root is a prefix of a path resolved Inside it");
        Ok((root, rel.to_path_buf()))
    }

    /// Open `normalized` (already confirmed `Inside` a mount by the caller)
    /// with the standard race-free resolve flags.
    fn open_contained(&self, normalized: &Path, flags: OFlags, mode: Mode) -> Result<OwnedFd, VfsError> {
        self.open_with(
            normalized,
            flags,
            mode,
            ResolveFlags::IN_ROOT | ResolveFlags::NO_MAGICLINKS,
        )
        .map_err(|source| self.io_err(normalized, source))
    }

    fn open_with(&self, normalized: &Path, flags: OFlags, mode: Mode, resolve: ResolveFlags) -> std::io::Result<OwnedFd> {
        let (root, rel) = self
            .split(normalized)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let root_fd = open_dir_no_follow(&root)?;
        let rel = if rel.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            rel
        };
        rfs::openat2(&root_fd, &rel, flags, mode, resolve).map_err(std::io::Error::from)
    }

    fn io_err(&self, path: &Path, source: impl Into<std::io::Error>) -> VfsError {
        let source = source.into();
        if source.kind() == std::io::ErrorKind::NotFound {
            VfsError::NotFound { path: path.to_path_buf() }
        } else {
            VfsError::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    }
}

fn open_dir_no_follow(path: &Path) -> std::io::Result<OwnedFd> {
    rfs::open(path, OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::mount::Mount;
    use std::os::unix::fs::symlink;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-harness-vfs-ops-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn join(&self, rel: &str) -> PathBuf {
            self.path.join(rel)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn table_with(root: &Path, mode: AccessMode) -> MountTable {
        MountTable::from_mounts(vec![Mount {
            root: root.to_path_buf(),
            mode,
        }])
    }

    #[test]
    fn write_then_read_round_trips_inside_a_read_write_mount() {
        let tmp = TempDir::new("rw");
        let vfs = Vfs::new(table_with(&tmp.path, AccessMode::ReadWrite));
        let file = tmp.join("hello.txt");
        vfs.write(&file, b"hello sandbox").unwrap();
        assert_eq!(vfs.read(&file).unwrap(), b"hello sandbox");
    }

    #[test]
    fn write_into_a_read_only_mount_is_refused() {
        let tmp = TempDir::new("ro");
        let vfs = Vfs::new(table_with(&tmp.path, AccessMode::ReadOnly));
        let err = vfs.write(&tmp.join("nope.txt"), b"x").unwrap_err();
        assert!(matches!(err, VfsError::ReadOnly { .. }));
    }

    #[test]
    fn read_dir_on_a_scaffold_lists_only_the_mounted_child_never_the_real_siblings() {
        let tmp = TempDir::new("scaffold");
        std::fs::create_dir_all(tmp.join("a/b")).unwrap();
        std::fs::write(tmp.join("a/sibling.txt"), b"should stay invisible").unwrap();
        let vfs = Vfs::new(table_with(&tmp.join("a/b"), AccessMode::ReadWrite));
        let entries = vfs.read_dir(&tmp.join("a")).unwrap();
        assert_eq!(entries, vec![Entry::Dir("b".to_string())]);
    }

    #[test]
    fn read_dir_outside_every_mount_is_denied() {
        let tmp = TempDir::new("outside");
        std::fs::create_dir_all(tmp.join("mounted")).unwrap();
        std::fs::create_dir_all(tmp.join("other")).unwrap();
        let vfs = Vfs::new(table_with(&tmp.join("mounted"), AccessMode::ReadWrite));
        let err = vfs.read_dir(&tmp.join("other")).unwrap_err();
        assert!(matches!(err, VfsError::Outside { .. }));
    }

    #[test]
    fn a_symlink_inside_a_mount_pointing_outside_it_cannot_be_read() {
        let tmp = TempDir::new("symlink-escape");
        std::fs::create_dir_all(tmp.join("mounted")).unwrap();
        std::fs::create_dir_all(tmp.join("secret")).unwrap();
        std::fs::write(tmp.join("secret/f.txt"), b"top secret").unwrap();
        symlink(tmp.join("secret"), tmp.join("mounted/escape")).unwrap();

        let vfs = Vfs::new(table_with(&tmp.join("mounted"), AccessMode::ReadWrite));
        // Structurally dead: `RESOLVE_IN_ROOT` resolves the symlink's
        // absolute target relative to the mount root, not the real
        // filesystem root, so `mounted/escape/f.txt` looks for
        // `mounted/secret/f.txt`, which doesn't exist.
        let err = vfs.read(&tmp.join("mounted/escape/f.txt")).unwrap_err();
        assert!(matches!(err, VfsError::NotFound { .. } | VfsError::Io { .. }));
    }

    #[test]
    fn writing_onto_an_existing_symlink_is_refused_rather_than_followed() {
        let tmp = TempDir::new("symlink-write");
        std::fs::create_dir_all(tmp.join("mounted")).unwrap();
        std::fs::create_dir_all(tmp.join("secret")).unwrap();
        symlink(tmp.join("secret"), tmp.join("mounted/link")).unwrap();
        let vfs = Vfs::new(table_with(&tmp.join("mounted"), AccessMode::ReadWrite));
        let err = vfs.write(&tmp.join("mounted/link"), b"x").unwrap_err();
        assert!(matches!(err, VfsError::SymlinkTarget { .. } | VfsError::Io { .. }));
    }

    #[test]
    fn metadata_reports_a_scaffold_as_an_empty_directory_without_touching_disk() {
        let tmp = TempDir::new("meta-scaffold");
        std::fs::create_dir_all(tmp.join("a/b")).unwrap();
        let vfs = Vfs::new(table_with(&tmp.join("a/b"), AccessMode::ReadWrite));
        let meta = vfs.metadata(&tmp.join("a")).unwrap();
        assert!(meta.is_dir);
        assert!(!meta.is_file);
    }

    #[test]
    fn create_dir_all_then_remove_file_round_trip() {
        let tmp = TempDir::new("mkdir-rm");
        let vfs = Vfs::new(table_with(&tmp.path, AccessMode::ReadWrite));
        let nested = tmp.join("x/y/z");
        vfs.create_dir_all(&nested).unwrap();
        let file = nested.join("f.txt");
        vfs.write(&file, b"data").unwrap();
        assert_eq!(vfs.read(&file).unwrap(), b"data");
        vfs.remove_file(&file).unwrap();
        assert!(matches!(vfs.metadata(&file), Err(VfsError::NotFound { .. }) | Err(VfsError::Io { .. })));
    }
}
