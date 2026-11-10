use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use level_core::Document;

use crate::{LoadError, read, write};

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Load { path: PathBuf, source: LoadError },
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> FileError + '_ {
    |source| FileError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn load_file(path: &Path) -> Result<Document, FileError> {
    let text = fs::read_to_string(path).map_err(io_err(path))?;
    read(&text).map_err(|source| FileError::Load {
        path: path.to_path_buf(),
        source,
    })
}

pub fn save_file(path: &Path, doc: &Document) -> Result<(), FileError> {
    write_atomic(path, write(doc).as_bytes())
}

/// Writes through a temporary sibling and renames it into place.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.tmp{}", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io_err(path))
}
