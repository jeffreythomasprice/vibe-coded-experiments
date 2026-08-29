/// Everything that can go wrong building or running a sandboxed command.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The configured backend doesn't actually work on this machine (or
    /// sandboxing is disabled outright). Returned instead of ever silently
    /// running unconfined — see `lib::sandbox::Disabled`.
    #[error("sandboxing is unavailable: {reason}")]
    Unavailable { reason: String },

    #[error("failed to spawn sandboxed process: {source}")]
    Spawn {
        #[source]
        source: std::io::Error,
    },
}
