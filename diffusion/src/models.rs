use std::path::{Path, PathBuf};

use hf_hub::api::sync::{ApiBuilder, ApiError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("failed to download {file} from {repo}: {source}")]
    Download {
        repo: String,
        file: String,
        #[source]
        source: ApiError,
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
