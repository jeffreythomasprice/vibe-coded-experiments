use std::io;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum ProtocolError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("stream closed before a full message was read")]
    StreamClosed,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("config file not found (searched: {searched:?})")]
    NotFound { searched: Vec<PathBuf> },
    #[error("reading config {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("parsing config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("secrets file not found (searched: {searched:?})")]
    SecretsNotFound { searched: Vec<PathBuf> },
    #[error("reading secrets {path}: {source}")]
    SecretsRead { path: PathBuf, source: io::Error },
    #[error("parsing secrets {path}: {source}")]
    SecretsParse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum ServerStartError {
    #[error("server already running on {socket}")]
    AlreadyRunning { socket: PathBuf },
    #[error("failed to bind {socket}: {source}")]
    Bind { socket: PathBuf, source: io::Error },
    #[error("failed to remove stale socket {socket}: {source}")]
    RemoveStale { socket: PathBuf, source: io::Error },
    #[error("failed to ensure socket parent directory {dir}: {source}")]
    ParentDir { dir: PathBuf, source: io::Error },
}

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("timed out waiting for server to accept connections on {socket}")]
    ConnectTimeout { socket: PathBuf },
    #[error("timed out waiting for response from server")]
    RequestTimeout,
    #[error("failed to spawn server process: {0}")]
    SpawnServer(#[source] io::Error),
    #[error("server responded with error: {message}")]
    ServerError { message: String },
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Top-level error that `main` turns into an exit code + JSON stderr line.
#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    ServerStart(#[from] ServerStartError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Logging(#[from] crate::logging::LoggingError),
    #[error(transparent)]
    Llm(#[from] crate::llm::LlmError),
    #[error(transparent)]
    Db(#[from] crate::db::DbError),
    #[error(transparent)]
    Tui(#[from] crate::tui::error::TuiError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::ServerStart(ServerStartError::AlreadyRunning { .. }) => 10,
            CliError::Client(ClientError::ConnectTimeout { .. }) => 11,
            CliError::Client(ClientError::RequestTimeout) => 12,
            CliError::Config(
                ConfigError::NotFound { .. } | ConfigError::SecretsNotFound { .. },
            ) => 13,
            CliError::Llm(crate::llm::LlmError::MissingSecret(_)) => 13,
            CliError::Db(crate::db::DbError::Open { .. } | crate::db::DbError::Migrate { .. }) => {
                14
            }
            _ => 1,
        }
    }

    /// One-line JSON suitable for stderr so callers can parse it machine-side.
    pub fn to_json_line(&self) -> String {
        let value = match self {
            CliError::ServerStart(ServerStartError::AlreadyRunning { socket }) => {
                serde_json::json!({ "error": "AlreadyRunning", "socket": socket })
            }
            CliError::ServerStart(ServerStartError::Bind { socket, source }) => {
                serde_json::json!({ "error": "Bind", "socket": socket, "message": source.to_string() })
            }
            CliError::ServerStart(ServerStartError::RemoveStale { socket, source }) => {
                serde_json::json!({ "error": "RemoveStaleSocket", "socket": socket, "message": source.to_string() })
            }
            CliError::ServerStart(ServerStartError::ParentDir { dir, source }) => {
                serde_json::json!({ "error": "SocketParentDir", "dir": dir, "message": source.to_string() })
            }
            CliError::Client(ClientError::ConnectTimeout { socket }) => {
                serde_json::json!({ "error": "ConnectTimeout", "socket": socket })
            }
            CliError::Client(ClientError::RequestTimeout) => {
                serde_json::json!({ "error": "RequestTimeout" })
            }
            CliError::Client(ClientError::SpawnServer(source)) => {
                serde_json::json!({ "error": "SpawnServer", "message": source.to_string() })
            }
            CliError::Client(ClientError::ServerError { message }) => {
                serde_json::json!({ "error": "ServerError", "message": message })
            }
            CliError::Client(other) => {
                serde_json::json!({ "error": "Client", "message": other.to_string() })
            }
            CliError::Config(ConfigError::NotFound { searched }) => {
                serde_json::json!({ "error": "ConfigNotFound", "searched": searched })
            }
            CliError::Config(ConfigError::SecretsNotFound { searched }) => {
                serde_json::json!({ "error": "SecretsNotFound", "searched": searched })
            }
            CliError::Config(ConfigError::SecretsRead { path, source }) => {
                serde_json::json!({ "error": "SecretsRead", "path": path, "message": source.to_string() })
            }
            CliError::Config(ConfigError::SecretsParse { path, source }) => {
                serde_json::json!({ "error": "SecretsParse", "path": path, "message": source.to_string() })
            }
            CliError::Config(other) => {
                serde_json::json!({ "error": "Config", "message": other.to_string() })
            }
            CliError::Logging(err) => {
                serde_json::json!({ "error": "Logging", "message": err.to_string() })
            }
            CliError::Llm(crate::llm::LlmError::MissingSecret(name)) => {
                serde_json::json!({ "error": "Llm", "variant": "MissingSecret", "secret": name })
            }
            CliError::Llm(err) => {
                serde_json::json!({ "error": "Llm", "message": err.to_string() })
            }
            CliError::Db(crate::db::DbError::Open { path, source }) => {
                serde_json::json!({ "error": "Db", "variant": "Open", "path": path, "message": source.to_string() })
            }
            CliError::Db(crate::db::DbError::Migrate {
                version,
                name,
                source,
            }) => {
                serde_json::json!({ "error": "Db", "variant": "Migrate", "version": version, "name": name, "message": source.to_string() })
            }
            CliError::Db(crate::db::DbError::DimensionMismatch { expected, actual }) => {
                serde_json::json!({ "error": "Db", "variant": "DimensionMismatch", "expected": expected, "actual": actual })
            }
            CliError::Db(crate::db::DbError::NotFound { kind, id }) => {
                serde_json::json!({ "error": "Db", "variant": "NotFound", "kind": kind, "id": id })
            }
            CliError::Db(err) => {
                serde_json::json!({ "error": "Db", "message": err.to_string() })
            }
            CliError::Tui(err) => {
                serde_json::json!({ "error": "Tui", "message": err.to_string() })
            }
            CliError::Other(err) => {
                serde_json::json!({ "error": "Other", "message": err.to_string() })
            }
        };
        value.to_string()
    }
}
