use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tracing::warn;

use crate::config::{expand_tilde, SyncEntity};

#[derive(Debug)]
pub struct FileGroup {
    pub relative_name: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum ResolvedEntity {
    Files { groups: Vec<FileGroup> },
}

fn is_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

fn expand_path(raw: &str) -> Result<Vec<PathBuf>> {
    let expanded = expand_tilde(raw);
    let expanded_str = expanded.to_string_lossy().to_string();

    if is_glob(&expanded_str) {
        let mut paths = Vec::new();
        for entry in glob::glob(&expanded_str)
            .with_context(|| format!("Invalid glob pattern: {}", expanded_str))?
        {
            let path = entry.with_context(|| "Glob iteration error")?;
            paths.push(path);
        }
        if paths.is_empty() {
            warn!("Glob pattern matched no files: {}", raw);
        }
        Ok(paths)
    } else {
        Ok(vec![expanded])
    }
}

#[derive(Debug, PartialEq)]
enum PathType {
    File,
    Directory,
    Missing,
}

fn classify(path: &Path) -> PathType {
    if path.is_dir() {
        PathType::Directory
    } else if path.is_file() {
        PathType::File
    } else {
        PathType::Missing
    }
}

pub fn resolve_entity(entity: &SyncEntity) -> Result<ResolvedEntity> {
    let mut all_paths: Vec<PathBuf> = Vec::new();

    for raw in &entity.paths {
        let expanded = expand_path(raw)?;
        all_paths.extend(expanded);
    }

    if all_paths.is_empty() {
        bail!(
            "Entity '{}': no paths resolved (all globs empty?)",
            entity.name
        );
    }

    let classifications: Vec<(PathBuf, PathType)> = all_paths
        .into_iter()
        .map(|p| {
            let t = classify(&p);
            (p, t)
        })
        .collect();

    let has_files = classifications.iter().any(|(_, t)| *t == PathType::File);
    let has_dirs = classifications.iter().any(|(_, t)| *t == PathType::Directory);

    if has_files && has_dirs {
        bail!(
            "Entity '{}': mixed file and directory paths are not supported",
            entity.name
        );
    }

    if has_dirs {
        // Directory mode
        let dir_paths: Vec<PathBuf> = classifications.into_iter().map(|(p, _)| p).collect();
        resolve_directories(&entity.name, dir_paths)
    } else {
        // File mode (including missing paths)
        let paths: Vec<PathBuf> = classifications.into_iter().map(|(p, _)| p).collect();
        Ok(ResolvedEntity::Files {
            groups: vec![FileGroup {
                relative_name: paths
                    .first()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .unwrap_or_default(),
                paths,
            }],
        })
    }
}

fn resolve_directories(entity_name: &str, dir_paths: Vec<PathBuf>) -> Result<ResolvedEntity> {
    // Build union of all relative paths across all directories
    let mut all_relative: BTreeSet<String> = BTreeSet::new();

    for dir in &dir_paths {
        if dir.is_dir() {
            collect_relative_paths(dir, dir, &mut all_relative)?;
        }
    }

    if all_relative.is_empty() {
        warn!("Entity '{}': all directories are empty", entity_name);
    }

    let groups: Vec<FileGroup> = all_relative
        .into_iter()
        .map(|rel| {
            let paths: Vec<PathBuf> = dir_paths.iter().map(|d| d.join(&rel)).collect();
            FileGroup {
                relative_name: rel,
                paths,
            }
        })
        .collect();

    Ok(ResolvedEntity::Files { groups })
}

fn collect_relative_paths(
    base: &Path,
    current: &Path,
    out: &mut BTreeSet<String>,
) -> Result<()> {
    let entries = std::fs::read_dir(current)
        .with_context(|| format!("Failed to read directory: {}", current.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_relative_paths(base, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .with_context(|| "Failed to strip prefix")?;
            out.insert(rel.to_string_lossy().to_string());
        }
    }
    Ok(())
}
