pub mod client;
pub mod config;
pub mod daemon;
pub mod modelset;
pub mod progress;
pub mod release;

use std::path::PathBuf;

use thiserror::Error;

pub use config::SdConfig;
pub use modelset::ModelSet;

#[derive(Debug, Error)]
pub enum SdError {
    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: Box<reqwest::Error>,
    },

    #[error("{url} returned {status}: {body}")]
    Status { status: u16, url: String, body: String },

    #[error("failed to parse the response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("no sd-server release asset for os={os} arch={arch} backend={backend}")]
    NoAssetForPlatform {
        os: String,
        arch: String,
        backend: String,
    },

    #[error("job {id} failed ({code}): {message}")]
    JobFailed { id: String, code: String, message: String },

    #[error("job {id} did not finish within {waited:?}")]
    JobTimeout { id: String, waited: std::time::Duration },

    #[error("job {id} is gone (410): the server no longer has its result")]
    JobGone { id: String },

    #[error("failed to decode image {index} in the job result: {source}")]
    ImageDecode {
        index: usize,
        #[source]
        source: base64::DecodeError,
    },

    #[error("expected {expected} image(s) in the job result but found {found}")]
    ImageCountMismatch { expected: usize, found: usize },

    #[error("failed to download {url}: {source}")]
    Download {
        url: String,
        #[source]
        source: Box<reqwest::Error>,
    },

    #[error("failed to extract {asset}: {source}")]
    Extract {
        asset: String,
        #[source]
        source: Box<zip::result::ZipError>,
    },

    #[error("downloaded {actual} bytes for {url}, expected {expected}")]
    SizeMismatch { url: String, expected: u64, actual: u64 },

    #[error("expected {} after extracting the sd-server release, but it is missing", .0.display())]
    MissingBinary(PathBuf),

    #[error("failed to {action} {}: {source}", .path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to spawn {}: {source}", .binary.display())]
    Spawn {
        binary: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("sd-server did not become ready within {waited:?}; last log output:\n{log}")]
    StartupTimeout { waited: std::time::Duration, log: String },

    #[error("sd-server exited during startup (code {code:?}); last log output:\n{log_tail}")]
    ServerExited { code: Option<i32>, log_tail: String },

    #[error(
        "{addr} is already in use by a process that isn't an image-gen-managed sd-server; \
         stop it or change [sd_server].port"
    )]
    PortOccupied { addr: String },

    #[error("timed out waiting {} for the sd-server startup lock at {}", format!("{:?}", .waited), .path.display())]
    LockTimeout { path: PathBuf, waited: std::time::Duration },

    #[error("--backend {backend} was requested, but the downloaded sd-server release ({tag}) does not support it")]
    BackendUnavailable { backend: &'static str, tag: String },
}
