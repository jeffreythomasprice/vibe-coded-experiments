use std::path::Path;

use image::DynamicImage;

use crate::error::AppError;

#[allow(dead_code)]
pub fn load(path: &Path) -> Result<DynamicImage, AppError> {
    Ok(image::open(path)?)
}

#[allow(dead_code)]
pub fn save(image: &DynamicImage, path: &Path) -> Result<(), AppError> {
    image.save(path)?;
    Ok(())
}

/// Prints `image` to stdout using whatever protocol the current terminal
/// supports (Kitty, iTerm, Sixel), falling back to half-block characters.
#[allow(dead_code)]
pub fn display(image: &DynamicImage) -> Result<(), AppError> {
    let config = viuer::Config::default();
    viuer::print(image, &config)?;
    Ok(())
}
