use std::path::{Path, PathBuf};
use std::str::FromStr;

use hf_hub::api::sync::{ApiBuilder, ApiError, ApiRepo};
use hf_hub::{Cache, Repo, RepoType};
use thiserror::Error;

static WEIGHT_EXTENSIONS: [&str; 4] = ["safetensors", "gguf", "ckpt", "pt"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRef {
    Local(PathBuf),
    Hub {
        repo: String,
        revision: Option<String>,
        file: Option<String>,
    },
}

impl FromStr for ModelRef {
    type Err = ModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.starts_with('/')
            || value.starts_with("./")
            || value.starts_with("../")
            || value.starts_with('~')
        {
            return Ok(Self::Local(expand_home(value)));
        }

        if let Some((repo_part, file)) = value.split_once(':') {
            let (repo, revision) = split_revision(repo_part);
            return Ok(Self::Hub {
                repo: repo.to_owned(),
                revision,
                file: Some(file.to_owned()),
            });
        }

        if Path::new(value).exists() {
            return Ok(Self::Local(PathBuf::from(value)));
        }

        let (repo, revision) = split_revision(value);
        if is_bare_repo(repo) {
            return Ok(Self::Hub {
                repo: repo.to_owned(),
                revision,
                file: None,
            });
        }

        Ok(Self::Local(PathBuf::from(value)))
    }
}

fn split_revision(value: &str) -> (&str, Option<String>) {
    match value.split_once('@') {
        Some((repo, revision)) => (repo, Some(revision.to_owned())),
        None => (value, None),
    }
}

fn is_bare_repo(value: &str) -> bool {
    let Some((owner, repo)) = value.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !repo.is_empty()
        && !repo.contains('/')
        && owner.chars().all(is_repo_char)
        && repo.chars().all(is_repo_char)
}

fn is_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn expand_home(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix('~')
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest.strip_prefix('/').unwrap_or(rest));
    }
    PathBuf::from(value)
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("failed to build huggingface client: {source}")]
    ApiBuild {
        #[source]
        source: ApiError,
    },

    #[error("failed to fetch repo info for '{repo}': {source}")]
    RepoInfo {
        repo: String,
        #[source]
        source: ApiError,
    },

    #[error("failed to download {file} from {repo}: {source}")]
    Download {
        repo: String,
        file: String,
        #[source]
        source: ApiError,
    },

    #[error("'{repo}' has no weight file (looked for .safetensors, .gguf, .ckpt, .pt)")]
    NoWeightFile { repo: String },

    #[error("'{repo}' matches {count} weight files; pick one with owner/repo:file\n{candidates}")]
    AmbiguousWeightFile {
        repo: String,
        count: usize,
        candidates: String,
    },

    #[error("local model file not found: {}", .0.display())]
    MissingLocalFile(PathBuf),

    #[error("failed to read the model cache at {}: {source}", .path.display())]
    CacheScan {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[allow(dead_code)]
pub fn download(models_dir: &Path, repo: &str, file: &str) -> Result<PathBuf, ModelError> {
    let fail = |source| ModelError::Download {
        repo: repo.to_owned(),
        file: file.to_owned(),
        source,
    };

    ApiBuilder::new()
        .with_cache_dir(models_dir.to_path_buf())
        .build()
        .map_err(fail)?
        .model(repo.to_owned())
        .get(file)
        .map_err(fail)
}

pub fn resolve(models_dir: &Path, model_ref: &ModelRef) -> Result<PathBuf, ModelError> {
    match model_ref {
        ModelRef::Local(path) => {
            if path.is_file() {
                Ok(path.clone())
            } else {
                Err(ModelError::MissingLocalFile(path.clone()))
            }
        }
        ModelRef::Hub {
            repo,
            revision,
            file,
        } => {
            let api = ApiBuilder::new()
                .with_cache_dir(models_dir.to_path_buf())
                .build()
                .map_err(|source| ModelError::ApiBuild { source })?;
            let api_repo = match revision {
                Some(revision) => api.repo(Repo::with_revision(
                    repo.clone(),
                    RepoType::Model,
                    revision.clone(),
                )),
                None => api.model(repo.clone()),
            };
            let file = match file {
                Some(file) => file.clone(),
                None => auto_resolve_file(repo, &api_repo)?,
            };
            api_repo.get(&file).map_err(|source| ModelError::Download {
                repo: repo.clone(),
                file,
                source,
            })
        }
    }
}

fn auto_resolve_file(repo: &str, api_repo: &ApiRepo) -> Result<String, ModelError> {
    let info = api_repo.info().map_err(|source| ModelError::RepoInfo {
        repo: repo.to_owned(),
        source,
    })?;

    let all_files: Vec<&str> = info.siblings.iter().map(|s| s.rfilename.as_str()).collect();
    let candidates = weight_candidates(&all_files);

    match candidates.as_slice() {
        [] => Err(ModelError::NoWeightFile {
            repo: repo.to_owned(),
        }),
        [single] => Ok((*single).to_owned()),
        many => {
            let count = many.len();
            let candidates = many
                .iter()
                .take(20)
                .map(|name| format!("  {name}"))
                .collect::<Vec<_>>()
                .join("\n");
            Err(ModelError::AmbiguousWeightFile {
                repo: repo.to_owned(),
                count,
                candidates,
            })
        }
    }
}

pub fn is_weight_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| WEIGHT_EXTENSIONS.contains(&ext))
}

/// Filters `files` down to weight files, preferring root-level files over ones nested
/// in subdirectories whenever at least one root-level weight file exists, and sorts
/// the result. This is the same rule `--model owner/repo` uses to auto-resolve a file
/// when no `:file` suffix is given.
pub fn weight_candidates<'a>(files: &[&'a str]) -> Vec<&'a str> {
    let all: Vec<&str> = files.iter().copied().filter(|name| is_weight_file(name)).collect();
    let root: Vec<&str> = all
        .iter()
        .copied()
        .filter(|name| !name.contains('/'))
        .collect();
    let mut candidates = if root.is_empty() { all } else { root };
    candidates.sort_unstable();
    candidates
}

/// The cached path for `repo`'s `file` on the `main` revision, or `None` if it hasn't
/// been downloaded into `models_dir`. Never touches the network.
pub fn cached_path(models_dir: &Path, repo: &str, file: &str) -> Option<PathBuf> {
    Cache::new(models_dir.to_path_buf())
        .model(repo.to_owned())
        .get(file)
}

/// One weight file found on disk under `models_dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedWeight {
    pub repo: String,
    pub file: String,
    pub size: u64,
}

/// Every weight file present under `models_dir`, one entry per `(repo, file)` pair
/// even if multiple cached revisions hold it.
pub fn cached_weights(models_dir: &Path) -> Result<Vec<CachedWeight>, ModelError> {
    let mut found = Vec::new();

    let entries = match std::fs::read_dir(models_dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(source) => {
            return Err(ModelError::CacheScan {
                path: models_dir.to_path_buf(),
                source,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|source| ModelError::CacheScan {
            path: models_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(repo_part) = name.strip_prefix("models--") else {
            continue;
        };
        // Repo IDs are `owner/repo`; hf-hub's folder_name() replaces the single `/`
        // with `--`, so undo that by restoring only the first occurrence. A repo
        // whose owner or name itself contains `--` would map back incorrectly, but
        // that mirrors the same ambiguity in hf-hub's own naming scheme.
        let repo = repo_part.replacen("--", "/", 1);

        let snapshots_dir = path.join("snapshots");
        let snapshot_entries = match std::fs::read_dir(&snapshots_dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(ModelError::CacheScan {
                    path: snapshots_dir,
                    source,
                });
            }
        };
        for snapshot in snapshot_entries {
            let snapshot = snapshot.map_err(|source| ModelError::CacheScan {
                path: snapshots_dir.clone(),
                source,
            })?;
            let snapshot_path = snapshot.path();
            if snapshot_path.is_dir() {
                collect_weight_files(&snapshot_path, &snapshot_path, &repo, &mut found)?;
            }
        }
    }

    found.sort_by(|a, b| (&a.repo, &a.file).cmp(&(&b.repo, &b.file)));
    found.dedup_by(|a, b| a.repo == b.repo && a.file == b.file);
    Ok(found)
}

fn collect_weight_files(
    root: &Path,
    dir: &Path,
    repo: &str,
    out: &mut Vec<CachedWeight>,
) -> Result<(), ModelError> {
    let entries = std::fs::read_dir(dir).map_err(|source| ModelError::CacheScan {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ModelError::CacheScan {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| ModelError::CacheScan {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_weight_files(root, &path, repo, out)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_weight_file(name) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Some(relative) = relative.to_str() else {
            continue;
        };
        let size = std::fs::metadata(&path)
            .map_err(|source| ModelError::CacheScan {
                path: path.clone(),
                source,
            })?
            .len();
        out.push(CachedWeight {
            repo: repo.to_owned(),
            file: relative.to_owned(),
            size,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_file_parses() {
        let parsed: ModelRef = "stabilityai/sdxl-turbo:sd_xl_turbo_1.0_fp16.safetensors"
            .parse()
            .unwrap();
        assert_eq!(
            parsed,
            ModelRef::Hub {
                repo: "stabilityai/sdxl-turbo".to_owned(),
                revision: None,
                file: Some("sd_xl_turbo_1.0_fp16.safetensors".to_owned()),
            }
        );
    }

    #[test]
    fn pinned_revision_parses() {
        let parsed: ModelRef = "stabilityai/sd-turbo@refs/pr/1:sd_turbo.safetensors"
            .parse()
            .unwrap();
        assert_eq!(
            parsed,
            ModelRef::Hub {
                repo: "stabilityai/sd-turbo".to_owned(),
                revision: Some("refs/pr/1".to_owned()),
                file: Some("sd_turbo.safetensors".to_owned()),
            }
        );
    }

    #[test]
    fn bare_repo_parses() {
        let parsed: ModelRef = "stabilityai/sd-turbo".parse().unwrap();
        assert_eq!(
            parsed,
            ModelRef::Hub {
                repo: "stabilityai/sd-turbo".to_owned(),
                revision: None,
                file: None,
            }
        );
    }

    #[test]
    fn dot_slash_forces_local() {
        let parsed: ModelRef = "./stabilityai/sd-turbo".parse().unwrap();
        assert_eq!(
            parsed,
            ModelRef::Local(PathBuf::from("./stabilityai/sd-turbo"))
        );
    }

    #[test]
    fn absolute_path_is_local() {
        let parsed: ModelRef = "/models/x.safetensors".parse().unwrap();
        assert_eq!(
            parsed,
            ModelRef::Local(PathBuf::from("/models/x.safetensors"))
        );
    }

    #[test]
    fn non_bare_string_is_local() {
        let parsed: ModelRef = "just-a-name".parse().unwrap();
        assert_eq!(parsed, ModelRef::Local(PathBuf::from("just-a-name")));
    }

    #[test]
    fn home_expands() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let parsed: ModelRef = "~/models/x.gguf".parse().unwrap();
        assert_eq!(parsed, ModelRef::Local(home.join("models/x.gguf")));
    }

    #[test]
    fn weight_candidates_filters_non_weight_files() {
        let files = ["README.md", "model.safetensors", "config.json"];
        assert_eq!(weight_candidates(&files), vec!["model.safetensors"]);
    }

    #[test]
    fn weight_candidates_prefers_root_over_nested() {
        let files = [
            "unet/diffusion_pytorch_model.safetensors",
            "model.safetensors",
            "vae/diffusion_pytorch_model.safetensors",
        ];
        assert_eq!(weight_candidates(&files), vec!["model.safetensors"]);
    }

    #[test]
    fn weight_candidates_falls_back_to_nested_when_no_root_file_exists() {
        let files = [
            "vae/diffusion_pytorch_model.safetensors",
            "unet/diffusion_pytorch_model.safetensors",
        ];
        assert_eq!(
            weight_candidates(&files),
            vec![
                "unet/diffusion_pytorch_model.safetensors",
                "vae/diffusion_pytorch_model.safetensors",
            ]
        );
    }

    #[test]
    fn weight_candidates_sorts_results() {
        let files = ["b.safetensors", "a.safetensors"];
        assert_eq!(
            weight_candidates(&files),
            vec!["a.safetensors", "b.safetensors"]
        );
    }

    fn write_fake_weight(models_dir: &Path, repo_folder: &str, snapshot: &str, file: &str, bytes: &[u8]) {
        let dir = models_dir.join(repo_folder).join("snapshots").join(snapshot);
        let file_path = dir.join(file);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, bytes).unwrap();
    }

    #[test]
    fn cached_weights_finds_root_and_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        write_fake_weight(
            dir.path(),
            "models--stabilityai--sd-turbo",
            "abc123",
            "sd_turbo.safetensors",
            b"12345",
        );
        write_fake_weight(
            dir.path(),
            "models--stabilityai--sdxl-turbo",
            "def456",
            "unet/diffusion_pytorch_model.safetensors",
            b"1234567890",
        );

        let mut found = cached_weights(dir.path()).unwrap();
        found.sort_by(|a, b| a.repo.cmp(&b.repo));

        assert_eq!(
            found,
            vec![
                CachedWeight {
                    repo: "stabilityai/sd-turbo".to_owned(),
                    file: "sd_turbo.safetensors".to_owned(),
                    size: 5,
                },
                CachedWeight {
                    repo: "stabilityai/sdxl-turbo".to_owned(),
                    file: "unet/diffusion_pytorch_model.safetensors".to_owned(),
                    size: 10,
                },
            ]
        );
    }

    #[test]
    fn cached_weights_dedups_across_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        write_fake_weight(
            dir.path(),
            "models--stabilityai--sd-turbo",
            "abc123",
            "sd_turbo.safetensors",
            b"hello",
        );
        write_fake_weight(
            dir.path(),
            "models--stabilityai--sd-turbo",
            "def456",
            "sd_turbo.safetensors",
            b"hello",
        );

        let found = cached_weights(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn cached_weights_ignores_non_repo_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".hub-cache")).unwrap();
        std::fs::write(dir.path().join(".hub-cache/search-1.json"), b"{}").unwrap();
        std::fs::create_dir_all(dir.path().join("logs")).unwrap();

        let found = cached_weights(dir.path()).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn cached_weights_on_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert_eq!(cached_weights(&missing).unwrap(), vec![]);
    }

    #[test]
    fn cached_path_is_none_when_not_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            cached_path(dir.path(), "stabilityai/sd-turbo", "sd_turbo.safetensors"),
            None
        );
    }
}
