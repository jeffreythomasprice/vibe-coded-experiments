use std::path::Path;

use crate::error::AppError;

pub fn display(path: &Path) -> Result<(), AppError> {
    let image = image::open(path)?;
    let config = viuer::Config {
        absolute_offset: false,
        ..Default::default()
    };
    viuer::print(&image, &config)?;
    Ok(())
}

/// Print the image's absolute path, then display it. Used whenever the image also
/// lives at a real, non-temporary file so the user can find it afterwards.
pub fn display_labeled(path: &Path) -> Result<(), AppError> {
    println!("{}", std::path::absolute(path)?.display());
    display(path)
}
