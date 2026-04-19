use std::path::{Path, PathBuf};

use directories::ProjectDirs;

pub fn socket_path(socket_dir: &Path) -> PathBuf {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    socket_dir.join(format!("llm-rag-{user}.sock"))
}

pub fn default_config_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("config.toml")];
    if let Some(project) = ProjectDirs::from("", "", "llm-rag") {
        paths.push(project.config_dir().join("config.toml"));
    }
    paths
}

pub fn default_secrets_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("secrets.toml")];
    if let Some(project) = ProjectDirs::from("", "", "llm-rag") {
        paths.push(project.config_dir().join("secrets.toml"));
    }
    paths
}
