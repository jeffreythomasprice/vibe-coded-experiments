//! Process-wide tracing/logging initialization.
//!
//! The default filter puts *our* crates at `trace` and everything else at
//! `warn`. "Our" crates are discovered at build time by `build.rs`, so this
//! stays correct as crates are added to or removed from the workspace.
//!
//! `RUST_LOG` (parsed via [`EnvFilter`]) overrides the default entirely, so
//! the usual `RUST_LOG=…` incantations work as expected.

use tracing_subscriber::{fmt, prelude::*, util::TryInitError, EnvFilter};

/// Comma-separated tracing target names for every crate in the workspace,
/// baked in by `build.rs` (e.g. `common,emulator,gameboy`).
const WORKSPACE_CRATE_TARGETS: &str = env!("WORKSPACE_CRATE_TARGETS");

#[derive(Debug, thiserror::Error)]
pub enum LogInitError {
    #[error("failed to install the global tracing subscriber (already initialized?)")]
    Init(#[from] TryInitError),
}

/// Install the global tracing subscriber.
///
/// Uses `RUST_LOG` when set, otherwise falls back to [`default_directives`].
/// Returns an error if a global subscriber has already been installed.
pub fn init() -> Result<(), LogInitError> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_directives()));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .try_init()?;

    Ok(())
}

/// The default `EnvFilter` directive string: everything at `warn`, with each
/// workspace crate bumped up to `trace`.
pub fn default_directives() -> String {
    let mut directives = String::from("warn");
    for target in WORKSPACE_CRATE_TARGETS.split(',').filter(|t| !t.is_empty()) {
        directives.push(',');
        directives.push_str(target);
        directives.push_str("=trace");
    }
    directives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_directives_default_everything_to_warn() {
        assert!(default_directives().starts_with("warn"));
    }

    #[test]
    fn default_directives_include_this_crate_at_trace() {
        // `common` is itself a workspace member, so build.rs must have found it.
        assert!(default_directives().contains("common=trace"));
    }

    #[test]
    fn default_directives_parse_as_a_valid_filter() {
        assert!(EnvFilter::try_new(default_directives()).is_ok());
    }
}
