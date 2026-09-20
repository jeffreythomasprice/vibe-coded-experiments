use std::path::{Path, PathBuf};

use crate::error::AppError;

/// The final path for each copy. `copies == 1` returns `base` unchanged; otherwise
/// each path gets a zero-padded index inserted before the extension.
pub fn destinations(base: &Path, copies: u32) -> Vec<PathBuf> {
    if copies == 1 {
        return vec![base.to_path_buf()];
    }

    let parent = base.parent().unwrap_or_else(|| Path::new(""));
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = base.extension().map(|e| e.to_string_lossy().into_owned());
    let width = (copies - 1).to_string().len();

    (0..copies)
        .map(|index| {
            let mut name = format!("{stem}{index:0width$}");
            if let Some(extension) = &extension {
                name.push('.');
                name.push_str(extension);
            }
            parent.join(name)
        })
        .collect()
}

/// Find the images `diffusion-rs` wrote for a batch generation into `dir`, sorted by
/// their numeric index. `diffusion-rs` names them `output_{date}_{id}.png` for `id`
/// in `1..=copies`, with a date it computes internally, so the names must be
/// discovered rather than predicted.
pub fn collect(dir: &Path, copies: u32) -> Result<Vec<PathBuf>, AppError> {
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter_map(|path| {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.rsplit('_').next())
                .and_then(|s| s.parse::<u32>().ok())?;
            Some((id, path))
        })
        .collect();
    found.sort_by_key(|(id, _)| *id);

    if found.len() != copies as usize {
        return Err(AppError::BatchOutputMismatch {
            expected: copies,
            found: found.len(),
            dir: dir.to_path_buf(),
        });
    }

    Ok(found.into_iter().map(|(_, path)| path).collect())
}

/// Move a generated image into its final destination. When `dest` keeps the PNG
/// `diffusion-rs` writes, this is a plain rename (falling back to copy+remove across
/// filesystems), preserving the EXIF generation-params tag. Any other extension is
/// re-encoded via `image`, which drops that tag.
pub fn place(src: &Path, dest: &Path) -> Result<(), AppError> {
    let is_png = match dest.extension() {
        Some(ext) => ext.eq_ignore_ascii_case("png"),
        None => true,
    };
    if is_png {
        if std::fs::rename(src, dest).is_ok() {
            return Ok(());
        }
        std::fs::copy(src, dest)?;
        std::fs::remove_file(src)?;
        return Ok(());
    }

    image::open(src)?.save(dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_copy_is_unchanged() {
        let base = PathBuf::from("/tmp/bike.png");
        assert_eq!(destinations(&base, 1), vec![base]);
    }

    #[test]
    fn five_copies_are_zero_indexed_unpadded() {
        let base = PathBuf::from("/tmp/bike.png");
        assert_eq!(
            destinations(&base, 5),
            vec![
                PathBuf::from("/tmp/bike0.png"),
                PathBuf::from("/tmp/bike1.png"),
                PathBuf::from("/tmp/bike2.png"),
                PathBuf::from("/tmp/bike3.png"),
                PathBuf::from("/tmp/bike4.png"),
            ]
        );
    }

    #[test]
    fn twelve_copies_are_padded_to_two_digits() {
        let base = PathBuf::from("/tmp/bike.png");
        let dests = destinations(&base, 12);
        assert_eq!(dests[0], PathBuf::from("/tmp/bike00.png"));
        assert_eq!(dests[9], PathBuf::from("/tmp/bike09.png"));
        assert_eq!(dests[11], PathBuf::from("/tmp/bike11.png"));
    }

    #[test]
    fn five_hundred_copies_are_padded_to_three_digits() {
        let base = PathBuf::from("/tmp/bike.png");
        let dests = destinations(&base, 500);
        assert_eq!(dests[0], PathBuf::from("/tmp/bike000.png"));
        assert_eq!(dests[499], PathBuf::from("/tmp/bike499.png"));
    }

    #[test]
    fn missing_extension_is_preserved() {
        let base = PathBuf::from("/tmp/bike");
        assert_eq!(
            destinations(&base, 3),
            vec![
                PathBuf::from("/tmp/bike0"),
                PathBuf::from("/tmp/bike1"),
                PathBuf::from("/tmp/bike2"),
            ]
        );
    }

    #[test]
    fn multi_dot_stem_only_splits_last_extension() {
        let base = PathBuf::from("/tmp/bike.final.png");
        assert_eq!(
            destinations(&base, 3)[0],
            PathBuf::from("/tmp/bike.final0.png")
        );
    }

    #[test]
    fn relative_path_has_no_parent() {
        let base = PathBuf::from("bike.png");
        assert_eq!(
            destinations(&base, 2),
            vec![PathBuf::from("bike0.png"), PathBuf::from("bike1.png"),]
        );
    }

    #[test]
    fn collect_sorts_numerically_past_nine() {
        let dir = tempfile::tempdir().unwrap();
        for id in [1, 2, 10, 11, 3] {
            std::fs::write(
                dir.path()
                    .join(format!("output_2026.01.01-00.00.00_{id}.png")),
                b"",
            )
            .unwrap();
        }

        let files = collect(dir.path(), 5).unwrap();
        let ids: Vec<u32> = files
            .iter()
            .map(|p| {
                p.file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .rsplit('_')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 10, 11]);
    }

    #[test]
    fn collect_errors_on_count_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("output_2026.01.01-00.00.00_1.png"), b"").unwrap();

        let err = collect(dir.path(), 3).unwrap_err();
        assert!(matches!(
            err,
            AppError::BatchOutputMismatch {
                expected: 3,
                found: 1,
                ..
            }
        ));
    }

    #[test]
    fn place_renames_png_to_png() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.png");
        let dest = dir.path().join("dest.png");
        std::fs::write(&src, b"fake png").unwrap();

        place(&src, &dest).unwrap();

        assert!(!src.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"fake png");
    }
}
