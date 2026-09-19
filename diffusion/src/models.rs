use std::path::{Path, PathBuf};
use std::str::FromStr;

use hf_hub::api::sync::{ApiBuilder, ApiError, ApiRepo};
use hf_hub::{Repo, RepoType};
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

    let is_weight_file = |name: &str| {
        Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| WEIGHT_EXTENSIONS.contains(&ext))
    };

    let all: Vec<&str> = info
        .siblings
        .iter()
        .map(|s| s.rfilename.as_str())
        .filter(|name| is_weight_file(name))
        .collect();
    let root: Vec<&str> = all
        .iter()
        .copied()
        .filter(|name| !name.contains('/'))
        .collect();
    let candidates = if root.is_empty() { all } else { root };

    match candidates.as_slice() {
        [] => Err(ModelError::NoWeightFile {
            repo: repo.to_owned(),
        }),
        [single] => Ok((*single).to_owned()),
        many => {
            let mut sorted = many.to_vec();
            sorted.sort_unstable();
            let count = sorted.len();
            let candidates = sorted
                .into_iter()
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
}
