use std::path::PathBuf;

use directories::{BaseDirs, ProjectDirs};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("could not determine the user's home directory")]
    NoHomeDir,
}

fn project_dirs() -> Result<ProjectDirs, PathsError> {
    ProjectDirs::from("", "", "audio-recorder").ok_or(PathsError::NoHomeDir)
}

pub fn home_dir() -> Result<PathBuf, PathsError> {
    BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .ok_or(PathsError::NoHomeDir)
}

pub fn user_config_file() -> Result<PathBuf, PathsError> {
    Ok(project_dirs()?.config_dir().join("config.toml"))
}

pub fn default_sessions_dir() -> Result<PathBuf, PathsError> {
    Ok(project_dirs()?.data_dir().join("sessions"))
}

pub fn default_log_file() -> PathBuf {
    std::env::temp_dir().join("audio-recorder.log")
}

pub fn expand_tilde(path: &str) -> Result<PathBuf, PathsError> {
    if path == "~" {
        return home_dir();
    }
    match path.strip_prefix("~/") {
        Some(rest) => Ok(home_dir()?.join(rest)),
        None => Ok(PathBuf::from(path)),
    }
}

pub fn device_state_file() -> Result<PathBuf, PathsError> {
    let dirs = project_dirs()?;
    let base = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Ok(base.join("devices.json"))
}

pub fn settings_file() -> Result<PathBuf, PathsError> {
    let dirs = project_dirs()?;
    let base = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Ok(base.join("settings.json"))
}

pub fn jobs_dir() -> Result<PathBuf, PathsError> {
    let dirs = project_dirs()?;
    let base = dirs.state_dir().unwrap_or_else(|| dirs.data_dir());
    Ok(base.join("jobs"))
}

pub fn default_models_dir() -> Result<PathBuf, PathsError> {
    Ok(project_dirs()?.data_dir().join("models"))
}
