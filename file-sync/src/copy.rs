use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::summary::Summary;

pub fn copy_file(
    src: &Path,
    dst: &Path,
    dry_run: bool,
    backup_dir: &Path,
    entity_name: &str,
    summary: &mut Summary,
) -> Result<()> {
    // Back up dst before overwriting (only if dst already exists)
    if dst.is_file() {
        let abs_dst = dst
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize {}", dst.display()))?;
        // Strip the leading `/` so we can join it onto the backup dir
        let relative = abs_dst
            .strip_prefix("/")
            .unwrap_or(&abs_dst);
        let backup_path = backup_dir.join(entity_name).join(relative);

        if dry_run {
            info!(
                "[dry-run] Would back up {} -> {}",
                dst.display(),
                backup_path.display()
            );
            summary.record_backup(dst, &backup_path, true);
        } else {
            if let Some(parent) = backup_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create backup parent dirs for {}",
                        backup_path.display()
                    )
                })?;
            }
            std::fs::copy(dst, &backup_path).with_context(|| {
                format!(
                    "Failed to back up {} -> {}",
                    dst.display(),
                    backup_path.display()
                )
            })?;
            info!("Backed up {} -> {}", dst.display(), backup_path.display());
            summary.record_backup(dst, &backup_path, false);
        }
    }

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
