use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::summary::Summary;

pub fn copy_file(src: &Path, dst: &Path, dry_run: bool, summary: &mut Summary) -> Result<()> {
    if dry_run {
        info!(
            "[dry-run] Would copy {} -> {}",
            src.display(),
            dst.display()
        );
        summary.record_copy(src, dst, true);
        return Ok(());
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent dirs for {}", dst.display()))?;
    }

    std::fs::copy(src, dst).with_context(|| {
        format!(
            "Failed to copy {} -> {}",
            src.display(),
            dst.display()
        )
    })?;

    info!("Copied {} -> {}", src.display(), dst.display());
    summary.record_copy(src, dst, false);
    Ok(())
}
