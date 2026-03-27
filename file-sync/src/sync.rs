use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::{error, info, warn};

use crate::copy::copy_file;
use crate::diff::build_conflict_info_multi;
use crate::resolve::FileGroup;
use crate::summary::Summary;
use crate::tui::app::UserChoice;
use crate::tui::run_diff_dialog;

/// Groups file indices by identical byte content. Returns a vec of groups,
/// where each group is a vec of indices into `contents`.
fn group_by_content(contents: &[Vec<u8>]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, data) in contents.iter().enumerate() {
        let mut found = false;
        for g in &mut groups {
            if contents[g[0]] == *data {
                g.push(i);
                found = true;
                break;
            }
        }
        if !found {
            groups.push(vec![i]);
        }
    }
    groups
}

pub fn sync_file_group(
    entity_name: &str,
    group: &FileGroup,
    dry_run: bool,
    verbose: bool,
    backup_dir: &Path,
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
                copy_file(src, dst, dry_run, backup_dir, entity_name, summary)?;
            }
        }
        _ => {
            // 2+ exist: group by identical content
            let contents: Vec<Vec<u8>> = {
                let mut v = Vec::with_capacity(existing.len());
                for p in &existing {
                    match fs::read(p) {
                        Ok(data) => v.push(data),
                        Err(e) => {
                            error!("Error reading {}: {}", p.display(), e);
                            summary.record_error(format!(
                                "{}: error reading {}: {}",
                                entity_name,
                                p.display(),
                                e
                            ));
                            return Ok(());
                        }
                    }
                }
                v
            };

            let groups = group_by_content(&contents);

            match groups.len() {
                1 => {
                    // All identical: just copy to missing locations
                    let first = existing[0];
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
                    } else {
                        if verbose {
                            println!(
                                "  [{}] in sync: '{}' ({} copies match, copying to {} missing)",
                                entity_name,
                                group.relative_name,
                                existing.len(),
                                missing.len()
                            );
                        }
                        for dst in &missing {
                            copy_file(first, dst, dry_run, backup_dir, entity_name, summary)?;
                        }
                    }
                }
                2..=5 => {
                    // Show N-column diff dialog with one representative per group
                    let representatives: Vec<&Path> = groups
                        .iter()
                        .map(|g| existing[g[0]].as_path())
                        .collect();
                    let rep_contents: Vec<&[u8]> = groups
                        .iter()
                        .map(|g| contents[g[0]].as_slice())
                        .collect();
                    let group_sizes: Vec<usize> = groups.iter().map(|g| g.len()).collect();

                    let conflict = build_conflict_info_multi(
                        entity_name,
                        &representatives,
                        &rep_contents,
                        &group_sizes,
                    )?;

                    let choice = run_diff_dialog(&conflict)?;

                    match choice {
                        UserChoice::Keep(group_idx) => {
                            if group_idx >= groups.len() {
                                warn!("Invalid choice index {}, skipping", group_idx);
                                summary.record_skip(format!(
                                    "{}: invalid choice for '{}'",
                                    entity_name, group.relative_name
                                ));
                                return Ok(());
                            }

                            let winner = existing[groups[group_idx][0]];
                            let losers: Vec<PathBuf> = groups
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| *i != group_idx)
                                .flat_map(|(_, g)| g.iter().map(|&idx| existing[idx].clone()))
                                .collect();

                            for dst in &losers {
                                copy_file(winner, dst, dry_run, backup_dir, entity_name, summary)?;
                            }
                            for dst in &missing {
                                copy_file(winner, dst, dry_run, backup_dir, entity_name, summary)?;
                            }

                            summary.record_conflict(entity_name, winner, losers);
                        }
                        UserChoice::Skip => {
                            info!(
                                "[{}] Skipped conflict for '{}'",
                                entity_name, group.relative_name
                            );
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
                n => {
                    // Too many unique versions — print error and skip
                    eprintln!(
                        "[{}] Too many unique versions ({}) for '{}', skipping.",
                        entity_name, n, group.relative_name
                    );
                    eprintln!("  Resolve manually. Groups:");
                    for (i, g) in groups.iter().enumerate() {
                        let paths: Vec<String> =
                            g.iter().map(|&idx| existing[idx].display().to_string()).collect();
                        eprintln!("    Group {}: {}", i + 1, paths.join(", "));
                    }
                    summary.record_skip(format!(
                        "{}: {} unique versions for '{}' (max 5)",
                        entity_name, n, group.relative_name
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
    use similar::ChangeTag;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn five_files_four_identical_third_different() {
        // Create 5 temp files: indices 0,1,3,4 identical, index 2 different
        let same_content = b"line one\nline two\nline three\n";
        let diff_content = b"line one\nchanged line\nline three\n";

        let mut files: Vec<NamedTempFile> = Vec::new();
        for i in 0..5 {
            let mut f = NamedTempFile::new().unwrap();
            if i == 2 {
                f.write_all(diff_content).unwrap();
            } else {
                f.write_all(same_content).unwrap();
            }
            f.flush().unwrap();
            files.push(f);
        }

        // Read contents (mirrors sync_file_group)
        let contents: Vec<Vec<u8>> = files
            .iter()
            .map(|f| fs::read(f.path()).unwrap())
            .collect();

        // Group by content
        let groups = group_by_content(&contents);

        // Should produce exactly 2 groups
        assert_eq!(groups.len(), 2);

        // Group 0 should have 4 members (the identical files: indices 0,1,3,4)
        assert_eq!(groups[0].len(), 4);
        assert_eq!(groups[0], vec![0, 1, 3, 4]);

        // Group 1 should have 1 member (the different file: index 2)
        assert_eq!(groups[1].len(), 1);
        assert_eq!(groups[1], vec![2]);

        // Build representatives and content slices (mirrors sync_file_group)
        let paths: Vec<&Path> = groups
            .iter()
            .map(|g| files[g[0]].path())
            .collect();
        let rep_contents: Vec<&[u8]> = groups
            .iter()
            .map(|g| contents[g[0]].as_slice())
            .collect();
        let group_sizes: Vec<usize> = groups.iter().map(|g| g.len()).collect();

        let conflict =
            build_conflict_info_multi("test-entity", &paths, &rep_contents, &group_sizes).unwrap();

        // Left text should be the majority content, right should be the different one
        assert_eq!(conflict.texts.len(), 2);
        let left_text = conflict.texts[0].as_ref().unwrap();
        let right_text = conflict.texts[1].as_ref().unwrap();
        assert_eq!(left_text, std::str::from_utf8(same_content).unwrap());
        assert_eq!(right_text, std::str::from_utf8(diff_content).unwrap());

        // Group sizes: 4 identical on left, 1 different on right
        assert_eq!(conflict.group_sizes, vec![4, 1]);

        // Build the App and check the diff rows show actual differences
        let app = App::new(&conflict);
        assert_eq!(app.num_copies, 2);

        // There should be rows with Delete/Insert tags (the changed line)
        let has_diff = app.rows.iter().any(|r| {
            r.columns[0]
                .as_ref()
                .map(|d| d.tag == ChangeTag::Delete)
                .unwrap_or(false)
                || r.columns[1]
                    .as_ref()
                    .map(|d| d.tag == ChangeTag::Insert)
                    .unwrap_or(false)
        });
        assert!(has_diff, "diff rows should highlight the changed line");
    }
}
