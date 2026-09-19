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
