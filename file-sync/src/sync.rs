use std::path::PathBuf;

use anyhow::Result;
use tracing::{error, info, warn};

use crate::copy::copy_file;
use crate::diff::{build_conflict_info, files_are_identical};
use crate::resolve::FileGroup;
use crate::summary::Summary;
use crate::tui::app::UserChoice;
use crate::tui::run_diff_dialog;

pub fn sync_file_group(
    entity_name: &str,
    group: &FileGroup,
    dry_run: bool,
    verbose: bool,
    summary: &mut Summary,
) -> Result<()> {
    let existing: Vec<&PathBuf> = group.paths.iter().filter(|p| p.is_file()).collect();
    let missing: Vec<&PathBuf> = group.paths.iter().filter(|p| !p.exists()).collect();

    match existing.len() {
        0 => {
            warn!(
                "[{}] No copies exist for '{}', skipping",
                entity_name, group.relative_name
            );
            if verbose {
                println!(
                    "  [{}] skip: no copies exist for '{}'",
                    entity_name, group.relative_name
                );
            }
            summary.record_skip(format!(
                "{}: no copies exist for '{}'",
                entity_name, group.relative_name
            ));
        }
        1 => {
            // Only one exists: copy to all missing
            let src = existing[0];
            if missing.is_empty() {
                if verbose {
                    println!(
                        "  [{}] in sync: {}",
                        entity_name,
                        src.display()
                    );
                }
                summary.record_in_sync();
            } else if verbose {
                for dst in &missing {
                    println!(
                        "  [{}] copy: {} -> {}",
                        entity_name,
                        src.display(),
                        dst.display()
                    );
                }
            }
            for dst in &missing {
                copy_file(src, dst, dry_run, summary)?;
            }
        }
        _ => {
            // 2+ exist: check if identical
            let first = existing[0];
            let mut all_identical = true;

            for other in &existing[1..] {
                match files_are_identical(first, other) {
                    Ok(true) => {}
                    Ok(false) => {
                        all_identical = false;
                        break;
                    }
                    Err(e) => {
                        error!("Error comparing files: {}", e);
                        summary.record_error(format!(
                            "{}: error comparing {} and {}: {}",
                            entity_name,
                            first.display(),
                            other.display(),
                            e
                        ));
                        return Ok(());
                    }
                }
            }

            if all_identical {
                // All identical: just copy to missing locations
                if missing.is_empty() {
                    if verbose {
                        println!(
                            "  [{}] in sync: '{}' ({} copies match)",
                            entity_name,
                            group.relative_name,
                            existing.len()
                        );
                    }
                    summary.record_in_sync();
                } else if verbose {
                    println!(
                        "  [{}] in sync: '{}' ({} copies match, copying to {} missing)",
                        entity_name,
                        group.relative_name,
                        existing.len(),
                        missing.len()
                    );
                }
                for dst in &missing {
                    copy_file(first, dst, dry_run, summary)?;
                }
            } else {
                // Conflict: show diff dialog for each pair
                // For simplicity, we compare first two differing copies
                let conflict =
                    build_conflict_info(entity_name, existing[0], existing[1])?;

                let choice = run_diff_dialog(&conflict)?;

                match choice {
                    UserChoice::Keep(index) => {
                        if index >= existing.len() {
                            warn!("Invalid choice index {}, skipping", index);
                            summary.record_skip(format!(
                                "{}: invalid choice for '{}'",
                                entity_name, group.relative_name
                            ));
                            return Ok(());
                        }

                        let winner = existing[index];
                        let losers: Vec<PathBuf> = existing
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != index)
                            .map(|(_, p)| (*p).clone())
                            .collect();

                        // Overwrite losers
                        for dst in &losers {
                            copy_file(winner, dst, dry_run, summary)?;
                        }
                        // Copy to missing
                        for dst in &missing {
                            copy_file(winner, dst, dry_run, summary)?;
                        }

                        summary.record_conflict(entity_name, winner, losers);
                    }
                    UserChoice::Skip => {
                        info!("[{}] Skipped conflict for '{}'", entity_name, group.relative_name);
                        summary.record_skip(format!(
                            "{}: user skipped conflict for '{}'",
                            entity_name, group.relative_name
                        ));
                    }
                    UserChoice::Quit => {
                        info!("User requested quit");
                        return Err(anyhow::anyhow!("User quit"));
                    }
                }
            }
        }
    }

    Ok(())
}
