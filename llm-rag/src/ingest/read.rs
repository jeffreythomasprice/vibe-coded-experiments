use std::path::Path;

use super::error::IngestError;

/// Ingest cap on file size. Bigger files mean more chunks, and each chunk is a
/// sequential embedding round-trip. Bump this if you actually need it.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Bytes at the head of the file inspected for NUL to classify a file as
/// "obviously binary". Kept small so we don't pay for reading then rejecting.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// Read a file, verify it's ASCII/UTF-8 and not obviously binary, and return
/// its contents as a `String`. File extension is deliberately ignored — we
/// trust the bytes.
pub fn read_text_file(path: &Path) -> Result<String, IngestError> {
    let metadata = std::fs::metadata(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let size = metadata.len();
    if size > MAX_FILE_BYTES {
        return Err(IngestError::TooLarge {
            path: path.to_path_buf(),
            actual: size,
            max: MAX_FILE_BYTES,
        });
    }

    let bytes = std::fs::read(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if bytes.is_empty() {
        return Err(IngestError::NotTextFile {
            path: path.to_path_buf(),
            reason: "file is empty",
        });
    }

    let sniff = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if sniff.contains(&0) {
        return Err(IngestError::NotTextFile {
            path: path.to_path_buf(),
            reason: "contains NUL byte",
        });
    }

    String::from_utf8(bytes).map_err(|_| IngestError::NotTextFile {
        path: path.to_path_buf(),
        reason: "not valid UTF-8",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn reads_plain_ascii() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, b"hello world").unwrap();
        assert_eq!(read_text_file(&p).unwrap(), "hello world");
    }

    #[test]
    fn reads_utf8_unicode() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "こんにちは 🌸".as_bytes()).unwrap();
        assert_eq!(read_text_file(&p).unwrap(), "こんにちは 🌸");
    }

    #[test]
    fn rejects_nul_byte() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, b"text\0here").unwrap();
        let err = read_text_file(&p).unwrap_err();
        assert!(matches!(
            err,
            IngestError::NotTextFile {
                reason: "contains NUL byte",
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.bin");
        // 0xFF alone is not valid UTF-8 and has no NUL.
        fs::write(&p, [0xFFu8, 0xFE, 0xFD]).unwrap();
        let err = read_text_file(&p).unwrap_err();
        assert!(matches!(
            err,
            IngestError::NotTextFile {
                reason: "not valid UTF-8",
                ..
            }
        ));
    }

    #[test]
    fn rejects_empty() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, b"").unwrap();
        let err = read_text_file(&p).unwrap_err();
        assert!(matches!(
            err,
            IngestError::NotTextFile {
                reason: "file is empty",
                ..
            }
        ));
    }

    #[test]
    fn rejects_too_large() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.txt");
        let big = vec![b'a'; (MAX_FILE_BYTES + 1) as usize];
        fs::write(&p, &big).unwrap();
        let err = read_text_file(&p).unwrap_err();
        assert!(matches!(err, IngestError::TooLarge { .. }));
    }
}
