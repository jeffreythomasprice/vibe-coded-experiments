use axum::http::HeaderValue;
use axum::http::header::InvalidHeaderValue;
use std::net::{AddrParseError, SocketAddr};

const DEFAULT_ADDRESS: &str = "0.0.0.0:8001";
// `TraceLayer::new_for_http()`'s default request/response spans log at `debug`, not `info` — this
// target needs `debug` specifically or every request goes silent.
pub const DEFAULT_LOG: &str = "warn,server=info,shared=info,tower_http::trace=debug";
const DEFAULT_CORS_ORIGINS: &str = "https://exalted.jeffrey.lol,http://127.0.0.1:8000";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid ADDRESS {value:?}: {source}")]
    Address { value: String, source: AddrParseError },
    #[error("invalid CORS_ORIGINS entry {value:?}: {source}")]
    CorsOrigin { value: String, source: InvalidHeaderValue },
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

        Ok(Self { address, cors_origins })
    }
}
