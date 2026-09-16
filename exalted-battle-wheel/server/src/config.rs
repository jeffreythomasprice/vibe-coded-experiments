use axum::http::HeaderValue;
use axum::http::header::InvalidHeaderValue;
use std::net::{AddrParseError, SocketAddr};
use std::path::Path;

/// Baked in at compile time to this crate's own directory, not read from the process's current
/// directory -- `cargo run -p server` and the deployed binary can have different working
/// directories, but this path is always `server/.env` relative to the workspace.
pub const DOTENV_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.env");

/// What happened trying to load `server/.env`, so `main` can log it once a subscriber exists.
/// Missing is the normal case in the deployed image, which never has the file at all.
pub enum DotenvOutcome {
    Loaded,
    NotFound,
    Unreadable(dotenvy::Error),
}

/// Loads `server/.env` into the process environment, if present, before anything else touches
/// it -- must run before the tokio runtime (and therefore its worker threads) exists, since
/// mutating the environment concurrently with threads that might read it is a real hazard, not
/// just a hoop the 2024-edition `unsafe fn` signature makes you jump through. `dotenvy::from_path`
/// never overrides a variable already set, so real process env (what `dev.sh` exports, what k8s
/// injects) always wins over the file.
pub fn load_dotenv() -> DotenvOutcome {
    match dotenvy::from_path(Path::new(DOTENV_PATH)) {
        Ok(()) => DotenvOutcome::Loaded,
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => DotenvOutcome::NotFound,
        Err(error) => DotenvOutcome::Unreadable(error),
    }
}

const DEFAULT_ADDRESS: &str = "0.0.0.0:8001";
// `TraceLayer::new_for_http()`'s default request/response spans log at `debug`, not `info` — this
// target needs `debug` specifically or every request goes silent.
pub const DEFAULT_LOG: &str = "warn,server=info,shared=info,tower_http::trace=debug";
// `localhost` and `127.0.0.1` are different origins to a browser even though they reach the same
// server, so both are listed -- dev.sh prints the latter, but nothing stops a developer (or a
// browser autocompleting a URL) from using the former instead.
const DEFAULT_CORS_ORIGINS: &str = "https://exalted.jeffrey.lol,http://127.0.0.1:8000,http://localhost:8000";
const DEFAULT_ACCESS_CODES_TABLE: &str = "exalted-battle-wheel-access-codes";
const DEFAULT_ROOMS_TABLE: &str = "exalted-battle-wheel-rooms";
const DEFAULT_CONNECTIONS_TABLE: &str = "exalted-battle-wheel-websocket-connections";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid ADDRESS {value:?}: {source}")]
    Address { value: String, source: AddrParseError },
    #[error("invalid CORS_ORIGINS entry {value:?}: {source}")]
    CorsOrigin { value: String, source: InvalidHeaderValue },
    #[error("{key} must be set")]
    Missing { key: &'static str },
}

/// Read before the rest of `Config`, so a filter directive is available to set up tracing before
/// any other configuration error needs to be logged. Deliberately as lenient as the client's own
/// `EnvFilter::new` — an unparseable directive is dropped rather than treated as a startup error.
pub fn log_directives() -> String {
    std::env::var("LOG").unwrap_or_else(|_| DEFAULT_LOG.to_string())
}

pub struct Config {
    pub address: SocketAddr,
    pub cors_origins: Vec<HeaderValue>,
    pub access_codes_table: String,
    pub rooms_table: String,
    pub connections_table: String,
    pub dynamodb_endpoint: Option<String>,
    /// Signs and verifies room session tokens (see `sessions.rs`). Required rather than defaulted
    /// or generated at startup: an in-process default would invalidate every outstanding session
    /// on every restart, silently undoing the whole point of a session that's supposed to survive
    /// one. `server/.env` carries a throwaway value for local development.
    pub session_secret: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let address_raw = std::env::var("ADDRESS").unwrap_or_else(|_| DEFAULT_ADDRESS.to_string());
        let address =
            address_raw.parse().map_err(|source| ConfigError::Address { value: address_raw.clone(), source })?;

        let cors_origins_raw = std::env::var("CORS_ORIGINS").unwrap_or_else(|_| DEFAULT_CORS_ORIGINS.to_string());
        let cors_origins = cors_origins_raw
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(|origin| {
                HeaderValue::from_str(origin)
                    .map_err(|source| ConfigError::CorsOrigin { value: origin.to_string(), source })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let access_codes_table =
            std::env::var("ACCESS_CODES_TABLE").unwrap_or_else(|_| DEFAULT_ACCESS_CODES_TABLE.to_string());
        let rooms_table = std::env::var("ROOMS_TABLE").unwrap_or_else(|_| DEFAULT_ROOMS_TABLE.to_string());
        let connections_table = std::env::var("CONNECTIONS_TABLE").unwrap_or_else(|_| DEFAULT_CONNECTIONS_TABLE.to_string());

        // Empty counts as unset, so a manifest can declare the variable without pointing at a
        // local DynamoDB.
        let dynamodb_endpoint = std::env::var("DYNAMODB_ENDPOINT").ok().filter(|value| !value.is_empty());

        let session_secret =
            std::env::var("SESSION_SECRET").map_err(|_| ConfigError::Missing { key: "SESSION_SECRET" })?;

        Ok(Self { address, cors_origins, access_codes_table, rooms_table, connections_table, dynamodb_endpoint, session_secret })
    }
}
