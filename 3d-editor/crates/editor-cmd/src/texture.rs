use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use editor_core::{ImageFormat, TextureSource};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextureError {
    #[error("texture file {path} not found")]
    NotFound { path: String },
    #[error("relative texture path {path} needs the scene to be saved first")]
    RelativeWithoutScene { path: String },
    #[error("cannot read {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("{path} is not a png, jpeg, tga or bmp file")]
    UnsupportedFormat { path: String },
    #[error("cannot decode the image: {reason}")]
    Decode { reason: String },
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
    /// The file the bytes came from, when they came from one.
    pub file: Option<PathBuf>,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

pub fn image_format(format: ImageFormat) -> image::ImageFormat {
    match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Tga => image::ImageFormat::Tga,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
    }
}

pub fn format_of_path(path: &Path) -> Option<ImageFormat> {
    ImageFormat::from_extension(path.extension()?.to_str()?)
}

pub fn resolve_path(path: &str, scene_dir: Option<&Path>) -> Result<PathBuf, TextureError> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    scene_dir
        .map(|d| d.join(p))
        .ok_or_else(|| TextureError::RelativeWithoutScene { path: path.into() })
}

pub fn load(source: &TextureSource, scene_dir: Option<&Path>) -> Result<Loaded, TextureError> {
    match source {
        TextureSource::Embedded { format, data } => Ok(Loaded {
            bytes: data.0.clone(),
            format: *format,
            file: None,
            modified: None,
        }),
        TextureSource::Path { path } => {
            let file = resolve_path(path, scene_dir)?;
            let format = format_of_path(&file)
                .ok_or_else(|| TextureError::UnsupportedFormat { path: path.clone() })?;
            let bytes = std::fs::read(&file).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => TextureError::NotFound {
                    path: file.display().to_string(),
                },
                _ => TextureError::Read {
                    path: file.display().to_string(),
                    reason: e.to_string(),
                },
            })?;
            let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
            Ok(Loaded {
                bytes,
                format,
                file: Some(file),
                modified,
            })
        }
    }
}

pub fn size_of(bytes: &[u8], format: ImageFormat) -> Result<Size, TextureError> {
    let reader = image::ImageReader::with_format(Cursor::new(bytes), image_format(format));
    let (width, height) = reader.into_dimensions().map_err(decode_error)?;
    Ok(Size { width, height })
}

pub fn decode_rgba8(bytes: &[u8], format: ImageFormat) -> Result<(Size, Vec<u8>), TextureError> {
    let img = image::load_from_memory_with_format(bytes, image_format(format))
        .map_err(decode_error)?
        .to_rgba8();
    let size = Size {
        width: img.width(),
        height: img.height(),
    };
    Ok((size, img.into_raw()))
}

fn decode_error(e: image::ImageError) -> TextureError {
    TextureError::Decode {
        reason: e.to_string(),
    }
}

/// The size of a material's image, or why it cannot be read.
pub fn source_size(source: &TextureSource, scene_dir: Option<&Path>) -> Result<Size, TextureError> {
    let loaded = load(source, scene_dir)?;
    size_of(&loaded.bytes, loaded.format)
}

/// `target` written relative to the directory `base`, using `..` where needed. Both must be absolute.
pub fn relative_to(target: &Path, base: &Path) -> Option<String> {
    let target: Vec<Component> = target.components().collect();
    let base: Vec<Component> = base.components().collect();
    let common = target.iter().zip(&base).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for c in &target[common..] {
        out.push(c.as_os_str());
    }
    Some(out.to_string_lossy().replace('\\', "/"))
}

/// A relative texture path rewritten so it still names the same file from `new_dir`.
pub fn rebase(path: &str, old_dir: &Path, new_dir: &Path) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }
    let file = lexical_normalize(&old_dir.join(path));
    relative_to(&file, &lexical_normalize(new_dir)).unwrap_or_else(|| file.display().to_string())
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_climb_out() {
        assert_eq!(
            relative_to(Path::new("/a/tex/x.png"), Path::new("/a/levels")).as_deref(),
            Some("../tex/x.png")
        );
        assert_eq!(
            relative_to(Path::new("/a/levels/x.png"), Path::new("/a/levels")).as_deref(),
            Some("x.png")
        );
    }

    #[test]
    fn rebase_keeps_the_same_file() {
        assert_eq!(
            rebase(
                "../tex/x.png",
                Path::new("/a/levels"),
                Path::new("/a/other/deep")
            ),
            "../../tex/x.png"
        );
        assert_eq!(
            rebase("/abs/x.png", Path::new("/a"), Path::new("/b")),
            "/abs/x.png"
        );
    }
}
