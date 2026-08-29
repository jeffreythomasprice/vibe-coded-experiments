use std::path::PathBuf;

/// Everything that can go wrong building or using a project's virtual
/// filesystem.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("path is not absolute: {path:?}")]
    NotAbsolute { path: PathBuf },

    /// A `..` that would climb above `/`. Rejected outright rather than
    /// silently clamped to root — a normalizer that clamps instead of
    /// erroring is how path containment bugs get introduced.
    #[error("path escapes above the filesystem root: {path:?}")]
    EscapesRoot { path: PathBuf },

    #[error("project directory does not exist or is not accessible: {path:?}")]
    MissingDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("project directory is not a directory: {path:?}")]
    NotADirectory { path: PathBuf },

    /// Binding `/` itself as a project directory would defeat the whole
    /// feature — every path resolves `Inside`, nothing is ever `Outside`.
    #[error("the filesystem root `/` cannot be a project directory")]
    RootMount,

    #[error("path is outside every project directory: {path:?}")]
    Outside { path: PathBuf },

    #[error("project directory is read-only: {path:?}")]
    ReadOnly { path: PathBuf },

    #[error("path does not exist: {path:?}")]
    NotFound { path: PathBuf },

    /// A write target's final component is itself an existing symlink.
    /// Refused rather than followed — the read path re-resolves the
    /// canonical target and is safe to follow, but a write is exactly the
    /// operation a symlink-swap race is trying to redirect.
    #[error("refusing to follow or overwrite a symlink: {path:?}")]
    SymlinkTarget { path: PathBuf },

    #[error("I/O error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
