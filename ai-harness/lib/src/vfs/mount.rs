//! The pure, filesystem-free core of a project's virtual filesystem: turning
//! a project's directory list into a [`MountTable`], and answering "what does
//! this path resolve to" against it.
//!
//! Deliberately pure — nothing here (except [`MountTable::build`]'s
//! `canonicalize` call) touches `std::fs` — so every rule (normalization,
//! ordering, mode resolution, the exact scaffold semantics) is a `#[test]`,
//! not something that only shows up once real directories and a real sandbox
//! are involved. [`super::Vfs`] (which does touch the filesystem) and
//! `lib::sandbox::bwrap_argv` (which builds a subprocess's mount namespace)
//! both consume the *same* [`MountTable`], so the file tools and a shell
//! running inside the sandbox can never disagree about what exists.

use std::path::{Component, Path, PathBuf};

use shared::project::{AccessMode, ProjectDir};

use super::error::VfsError;

/// One normalized, absolute, symlink-resolved directory and the access it
/// grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub root: PathBuf,
    pub mode: AccessMode,
}

/// A project's whole virtual filesystem: a set of mounts, sorted so a parent
/// always precedes any child nested inside it — the order
/// `lib::sandbox::bwrap_argv` relies on, since binding a parent *after* a
/// child silently clobbers the child back to the parent's mode (verified
/// against a real `bwrap` run; see `lib::sandbox`'s module doc).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountTable {
    mounts: Vec<Mount>,
}

/// What a path resolves to against a [`MountTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A proper ancestor of one or more mounts. Synthetic: lists only the
    /// path components that lead somewhere real, and is never read from the
    /// actual filesystem — this is what makes `/a` show only `b` even if the
    /// real `/a` has other siblings, and what `/e` never appearing at all
    /// comes down to (it has no descendant mount, so it is `Outside`, not
    /// `Scaffold`).
    Scaffold { children: Vec<String> },
    /// Inside a mount. `mount` indexes [`MountTable::mounts`] — the deepest
    /// (most specific) mount containing the path supplies the mode, so a
    /// read-write directory nested inside a read-only one behaves the same
    /// way a nested `bwrap` bind does.
    Inside { mount: usize, mode: AccessMode },
    /// Not reachable at all. An empty table (the default project) resolves
    /// every path this way, including `/` itself — "zero directories" means
    /// no filesystem access, not an empty-but-visible root.
    Outside,
}

impl MountTable {
    /// Normalize, canonicalize, and validate `dirs` into a table. The only
    /// method on this type that touches the filesystem.
    pub fn build(dirs: &[ProjectDir]) -> Result<Self, VfsError> {
        let mut mounts = Vec::with_capacity(dirs.len());
        for dir in dirs {
            let normalized = normalize(&dir.path)?;
            let real = std::fs::canonicalize(&normalized).map_err(|source| VfsError::MissingDir {
                path: normalized.clone(),
                source,
            })?;
            if !real.is_dir() {
                return Err(VfsError::NotADirectory { path: real });
            }
            if real == Path::new("/") {
                return Err(VfsError::RootMount);
            }
            mounts.push(Mount {
                root: real,
                mode: dir.mode,
            });
        }
        Ok(Self::from_mounts(mounts))
    }

    /// Build directly from already-canonical mounts — the pure constructor
    /// every test in this module uses, and the one place normalization order
    /// and mode-conflict resolution are decided.
    pub fn from_mounts(mut mounts: Vec<Mount>) -> Self {
        // Parents before children: comparing component sequences
        // lexicographically puts a path before any path it's a prefix of,
        // which is exactly "parent before child" since a parent's components
        // are always a prefix of its children's. `resolve`'s longest-prefix
        // search doesn't actually depend on this order (it just tracks the
        // longest match), but `lib::sandbox::bwrap_argv` does: a `bwrap`
        // bind of a parent *after* its child silently clobbers the child
        // back to the parent's mode (verified against a real `bwrap` run —
        // see that module's doc), so the mounts must come out of here in
        // exactly this order.
        mounts.sort_by(|a, b| a.root.components().cmp(b.root.components()));

        // Dedupe on the canonical root. A later duplicate can only narrow
        // the mode (read-only wins) — an entry list is a union of grants,
        // so ambiguity about the same root resolves to the safe reading,
        // not to "whichever came last in the input".
        let mut deduped: Vec<Mount> = Vec::with_capacity(mounts.len());
        for mount in mounts {
            if let Some(existing) = deduped.iter_mut().find(|m| m.root == mount.root) {
                if mount.mode == AccessMode::ReadOnly {
                    existing.mode = AccessMode::ReadOnly;
                }
            } else {
                deduped.push(mount);
            }
        }
        Self { mounts: deduped }
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }

    /// Pure and lexical: `path` must already be absolute and normalized (see
    /// [`normalize`]) — this method does not touch the filesystem and cannot
    /// see through a symlink in `path` itself. Callers reading or writing a
    /// real file re-resolve the *canonicalized* path a second time — see
    /// `Vfs`'s module doc for why that second resolve is what actually
    /// closes the symlink-escape hole.
    pub fn resolve(&self, path: &Path) -> Resolution {
        let path_components: Vec<Component<'_>> = path.components().collect();

        // Longest-prefix match: any two mounts that both contain `path` must
        // be a prefix of one another (both agree with `path` on the
        // overlapping range), so simply tracking the longest match by
        // component count is correct regardless of table ordering.
        let mut best: Option<(usize, usize)> = None; // (mount index, component count)
        for (index, mount) in self.mounts.iter().enumerate() {
            let mount_components: Vec<Component<'_>> = mount.root.components().collect();
            if path_components.len() >= mount_components.len()
                && path_components[..mount_components.len()] == mount_components[..]
                && best.is_none_or(|(_, len)| mount_components.len() > len)
            {
                best = Some((index, mount_components.len()));
            }
        }
        if let Some((index, _)) = best {
            return Resolution::Inside {
                mount: index,
                mode: self.mounts[index].mode,
            };
        }

        // Not inside any mount — is it a strict ancestor of one? Collect the
        // next path component of every mount whose root starts with `path`.
        let mut children: Vec<String> = Vec::new();
        for mount in &self.mounts {
            let mount_components: Vec<Component<'_>> = mount.root.components().collect();
            if mount_components.len() > path_components.len()
                && mount_components[..path_components.len()] == path_components[..]
            {
                if let Component::Normal(name) = mount_components[path_components.len()] {
                    let name = name.to_string_lossy().into_owned();
                    if !children.contains(&name) {
                        children.push(name);
                    }
                }
            }
        }
        if children.is_empty() {
            Resolution::Outside
        } else {
            children.sort();
            Resolution::Scaffold { children }
        }
    }
}

/// Lexically normalize a path: must be absolute; `.` components are dropped;
/// repeated/trailing separators collapse (for free — `Path::components`
/// already does this); `..` pops the previous component and is a hard error
/// if there is nothing to pop, i.e. an attempt to climb above `/`. No
/// filesystem access — this does not resolve symlinks.
pub fn normalize(path: &Path) -> Result<PathBuf, VfsError> {
    if !path.is_absolute() {
        return Err(VfsError::NotAbsolute {
            path: path.to_path_buf(),
        });
    }
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push(component),
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) | None => {
                    return Err(VfsError::EscapesRoot {
                        path: path.to_path_buf(),
                    });
                }
                _ => {
                    out.pop();
                }
            },
            Component::Normal(_) => out.push(component),
            Component::Prefix(_) => {
                // Windows path prefixes (`C:\`) never occur on the
                // Linux-only target this module runs on (see this crate's
                // `lib::vfs`/`lib::sandbox` module docs); treated as invalid
                // input rather than silently dropped.
                return Err(VfsError::NotAbsolute {
                    path: path.to_path_buf(),
                });
            }
        }
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(path: &str, mode: AccessMode) -> ProjectDir {
        ProjectDir {
            path: PathBuf::from(path),
            mode,
        }
    }

    fn mount(path: &str, mode: AccessMode) -> Mount {
        Mount {
            root: PathBuf::from(path),
            mode,
        }
    }

    // --- normalize ---

    #[test]
    fn normalize_rejects_a_relative_path() {
        assert!(matches!(
            normalize(Path::new("a/b")),
            Err(VfsError::NotAbsolute { .. })
        ));
    }

    #[test]
    fn normalize_drops_current_dir_components() {
        assert_eq!(normalize(Path::new("/a/./b")).unwrap(), PathBuf::from("/a/b"));
    }

    #[test]
    fn normalize_resolves_parent_dir_components_lexically() {
        assert_eq!(normalize(Path::new("/a/b/../c")).unwrap(), PathBuf::from("/a/c"));
    }

    #[test]
    fn normalize_rejects_a_parent_dir_that_climbs_above_root() {
        assert!(matches!(
            normalize(Path::new("/a/../..")),
            Err(VfsError::EscapesRoot { .. })
        ));
        assert!(matches!(normalize(Path::new("/..")), Err(VfsError::EscapesRoot { .. })));
    }

    #[test]
    fn normalize_collapses_repeated_and_trailing_separators() {
        assert_eq!(normalize(Path::new("/a//b/")).unwrap(), PathBuf::from("/a/b"));
    }

    #[test]
    fn normalize_root_is_root() {
        assert_eq!(normalize(Path::new("/")).unwrap(), PathBuf::from("/"));
    }

    // --- from_mounts: ordering, dedupe, mode conflicts ---

    #[test]
    fn from_mounts_sorts_parents_before_children() {
        let table = MountTable::from_mounts(vec![
            mount("/a/b", AccessMode::ReadWrite),
            mount("/a", AccessMode::ReadWrite),
            mount("/d", AccessMode::ReadWrite),
        ]);
        let roots: Vec<&Path> = table.mounts().iter().map(|m| m.root.as_path()).collect();
        assert_eq!(roots, vec![Path::new("/a"), Path::new("/a/b"), Path::new("/d")]);
    }

    #[test]
    fn from_mounts_orders_a_true_ancestor_before_its_descendant_even_with_an_unrelated_lexical_neighbor_present() {
        // "/a-b" is not an ancestor or descendant of either other mount —
        // just a lexical near-neighbor of "/a" and "/a/b" — and must not
        // land between them in a way that could be mistaken for nesting.
        let table = MountTable::from_mounts(vec![
            mount("/a-b", AccessMode::ReadWrite),
            mount("/a/b", AccessMode::ReadWrite),
            mount("/a", AccessMode::ReadWrite),
        ]);
        let roots: Vec<&Path> = table.mounts().iter().map(|m| m.root.as_path()).collect();
        let a = roots.iter().position(|p| *p == Path::new("/a")).unwrap();
        let a_b = roots.iter().position(|p| *p == Path::new("/a/b")).unwrap();
        assert!(a < a_b, "ancestor /a must sort before descendant /a/b, got {roots:?}");
    }

    #[test]
    fn from_mounts_dedupes_a_repeated_root_and_the_more_restrictive_mode_wins() {
        let table = MountTable::from_mounts(vec![
            mount("/a", AccessMode::ReadWrite),
            mount("/a", AccessMode::ReadOnly),
        ]);
        assert_eq!(table.mounts().len(), 1);
        assert_eq!(table.mounts()[0].mode, AccessMode::ReadOnly);

        // Order of input must not matter.
        let table = MountTable::from_mounts(vec![
            mount("/a", AccessMode::ReadOnly),
            mount("/a", AccessMode::ReadWrite),
        ]);
        assert_eq!(table.mounts()[0].mode, AccessMode::ReadOnly);
    }

    #[test]
    fn from_mounts_keeps_nested_mounts_with_different_modes_as_two_entries() {
        let table = MountTable::from_mounts(vec![
            mount("/a", AccessMode::ReadOnly),
            mount("/a/b", AccessMode::ReadWrite),
        ]);
        assert_eq!(table.mounts().len(), 2);
    }

    // --- resolve: the scenario from the plan ---
    //
    // real:   /a/b/c.txt, /d, /e/f.txt   mounted: /a/b, /d
    // virtual: /a (scaffold: only "b"), /a/b/c.txt, /d (empty dir); no /e.

    fn scenario() -> MountTable {
        MountTable::from_mounts(vec![
            mount("/a/b", AccessMode::ReadWrite),
            mount("/d", AccessMode::ReadWrite),
        ])
    }

    #[test]
    fn resolve_scaffolds_a_synthetic_ancestor_listing_only_its_mounted_children() {
        assert_eq!(
            scenario().resolve(Path::new("/a")),
            Resolution::Scaffold {
                children: vec!["b".to_string()]
            }
        );
        assert_eq!(
            scenario().resolve(Path::new("/")),
            Resolution::Scaffold {
                children: vec!["a".to_string(), "d".to_string()]
            }
        );
    }

    #[test]
    fn resolve_finds_a_file_inside_a_mount() {
        assert_eq!(
            scenario().resolve(Path::new("/a/b/c.txt")),
            Resolution::Inside {
                mount: 0,
                mode: AccessMode::ReadWrite
            }
        );
    }

    #[test]
    fn resolve_treats_a_mounted_empty_directory_as_inside_and_present() {
        let table = scenario();
        assert_eq!(
            table.resolve(Path::new("/d")),
            Resolution::Inside {
                mount: 1,
                mode: AccessMode::ReadWrite
            }
        );
    }

    #[test]
    fn resolve_denies_anything_with_no_path_to_a_mount() {
        assert_eq!(scenario().resolve(Path::new("/e")), Resolution::Outside);
        assert_eq!(scenario().resolve(Path::new("/e/f.txt")), Resolution::Outside);
    }

    #[test]
    fn resolve_uses_the_deepest_mount_for_a_nested_pair_with_different_modes() {
        // Mirrors the real `bwrap` layering verified by hand: `--ro-bind /a`
        // then `--bind /a/b/out` makes `/a` read-only, `/a/b/out` writable,
        // and everything else under `/a` (including `/a/b` itself) ordinary
        // real content of the read-only mount — not a synthetic scaffold,
        // since an ancestor mount already covers it.
        let table = MountTable::from_mounts(vec![
            mount("/a", AccessMode::ReadOnly),
            mount("/a/b/out", AccessMode::ReadWrite),
        ]);
        assert_eq!(
            table.resolve(Path::new("/a/nope.txt")),
            Resolution::Inside {
                mount: 0,
                mode: AccessMode::ReadOnly
            }
        );
        assert_eq!(
            table.resolve(Path::new("/a/b/out/ok.txt")),
            Resolution::Inside {
                mount: 1,
                mode: AccessMode::ReadWrite
            }
        );
        // A sibling of the nested read-write mount is real content of the
        // covering read-only mount, not a scaffold.
        assert_eq!(
            table.resolve(Path::new("/a/b/sibling.txt")),
            Resolution::Inside {
                mount: 0,
                mode: AccessMode::ReadOnly
            }
        );
        assert_eq!(
            table.resolve(Path::new("/a/b")),
            Resolution::Inside {
                mount: 0,
                mode: AccessMode::ReadOnly
            }
        );
    }

    #[test]
    fn resolve_scaffolds_only_when_no_ancestor_mount_covers_the_path() {
        // With no mount covering `/a` itself, `/a` is a pure scaffold
        // pointing at the one real mount nested underneath it.
        let table = MountTable::from_mounts(vec![mount("/a/b/out", AccessMode::ReadWrite)]);
        assert_eq!(
            table.resolve(Path::new("/a")),
            Resolution::Scaffold {
                children: vec!["b".to_string()]
            }
        );
        assert_eq!(
            table.resolve(Path::new("/a/b")),
            Resolution::Scaffold {
                children: vec!["out".to_string()]
            }
        );
        assert_eq!(table.resolve(Path::new("/a/other")), Resolution::Outside);
    }

    #[test]
    fn an_empty_table_denies_every_path_including_root() {
        let table = MountTable::default();
        assert_eq!(table.resolve(Path::new("/")), Resolution::Outside);
        assert_eq!(table.resolve(Path::new("/anything")), Resolution::Outside);
        assert!(table.is_empty());
    }

    // --- build: filesystem-backed ---

    #[test]
    fn build_rejects_a_directory_that_does_not_exist() {
        let missing = std::env::temp_dir().join(format!(
            "ai-harness-vfs-test-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let err = MountTable::build(&[dir(missing.to_str().unwrap(), AccessMode::ReadWrite)]).unwrap_err();
        assert!(matches!(err, VfsError::MissingDir { .. }));
    }

    #[test]
    fn build_rejects_binding_the_filesystem_root() {
        let err = MountTable::build(&[dir("/", AccessMode::ReadWrite)]).unwrap_err();
        assert!(matches!(err, VfsError::RootMount));
    }
}
