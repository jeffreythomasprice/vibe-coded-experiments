use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
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
        let include_filter = build_include_filter(&entity.include)?;
        resolve_directories(&entity.name, dir_paths, include_filter.as_ref())
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

fn build_include_filter(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .with_context(|| format!("Invalid include glob pattern: {pattern}"))?;
        builder.add(glob);
    }
    let glob_set = builder
        .build()
        .with_context(|| "Failed to build include glob set")?;
    Ok(Some(glob_set))
}

fn resolve_directories(
    entity_name: &str,
    dir_paths: Vec<PathBuf>,
    include_filter: Option<&GlobSet>,
) -> Result<ResolvedEntity> {
    // Build union of all relative paths across all directories
    let mut all_relative: BTreeSet<String> = BTreeSet::new();

    for dir in &dir_paths {
        if dir.is_dir() {
            collect_relative_paths(dir, dir, &mut all_relative)?;
        }
    }

    // Apply include filter if specified
    if let Some(glob_set) = include_filter {
        all_relative.retain(|rel| glob_set.is_match(rel));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncEntity;
    use std::fs;

    /// Create a temp dir with the given file tree.
    /// `files` are relative paths like "setup.sh", "subdir/notes.txt".
    fn make_dir(base: &Path, files: &[&str]) {
        for file in files {
            let p = base.join(file);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, format!("content of {file}")).unwrap();
        }
    }

    fn group_names(entity: &ResolvedEntity) -> Vec<String> {
        match entity {
            ResolvedEntity::Files { groups } => {
                groups.iter().map(|g| g.relative_name.clone()).collect()
            }
        }
    }

    #[test]
    fn include_filters_to_sh_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");
        let dir_b = tmp.path().join("b");

        make_dir(
            &dir_a,
            &["setup.sh", "install.sh", "readme.txt", "config.toml"],
        );
        make_dir(
            &dir_b,
            &["deploy.sh", "notes.md", "setup.sh"],
        );

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![
                dir_a.to_string_lossy().into(),
                dir_b.to_string_lossy().into(),
            ],
            include: vec!["*.sh".into()],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        assert_eq!(names, vec!["deploy.sh", "install.sh", "setup.sh"]);
    }

    #[test]
    fn include_filters_nested_with_double_star() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");
        let dir_b = tmp.path().join("b");

        make_dir(
            &dir_a,
            &["top.sh", "bin/run.sh", "bin/helper.py", "lib/deep/script.sh"],
        );
        make_dir(&dir_b, &["bin/run.sh", "docs/readme.md"]);

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![
                dir_a.to_string_lossy().into(),
                dir_b.to_string_lossy().into(),
            ],
            include: vec!["**/*.sh".into()],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        // *.sh at top level is NOT matched by **/*.sh (requires at least one dir component)
        // but globset's **/ matches zero or more directories, so top.sh should match too
        assert!(names.contains(&"bin/run.sh".to_string()));
        assert!(names.contains(&"lib/deep/script.sh".to_string()));
        assert!(names.contains(&"top.sh".to_string()));
        assert!(!names.contains(&"bin/helper.py".to_string()));
        assert!(!names.contains(&"docs/readme.md".to_string()));
    }

    #[test]
    fn no_include_returns_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");

        make_dir(&dir_a, &["one.sh", "two.txt", "sub/three.rs"]);

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![dir_a.to_string_lossy().into()],
            include: vec![],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        assert_eq!(names, vec!["one.sh", "sub/three.rs", "two.txt"]);
    }

    #[test]
    fn include_multiple_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");

        make_dir(
            &dir_a,
            &["script.sh", "config.toml", "notes.md", "lib.rs"],
        );

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![dir_a.to_string_lossy().into()],
            include: vec!["*.sh".into(), "*.toml".into()],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        assert_eq!(names, vec!["config.toml", "script.sh"]);
    }

    #[test]
    fn include_matching_nothing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");

        make_dir(&dir_a, &["file.txt", "file.rs"]);

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![dir_a.to_string_lossy().into()],
            include: vec!["*.py".into()],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        assert!(names.is_empty());
    }

    #[test]
    fn star_matches_across_directories_in_globset() {
        // globset's `*` matches across `/` — this differs from filesystem globs.
        // So `*.sh` matches both top-level and nested .sh files.
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");

        make_dir(&dir_a, &["top.sh", "sub/nested.sh", "sub/notes.txt"]);

        let entity = SyncEntity {
            name: "test".into(),
            paths: vec![dir_a.to_string_lossy().into()],
            include: vec!["*.sh".into()],
        };

        let resolved = resolve_entity(&entity).unwrap();
        let names = group_names(&resolved);

        assert_eq!(names, vec!["sub/nested.sh", "top.sh"]);
    }
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
