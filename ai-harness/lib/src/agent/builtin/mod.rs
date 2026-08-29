//! The tools shipped with this build: a virtual-filesystem suite plus
//! sandboxed `bash`, both riding on the mechanism `lib::vfs`/`lib::sandbox`
//! already provide — see those modules' docs.
//!
//! [`registry`] is what `Service::from_config` registers; `Service::new`
//! (used by every other unit test in this workspace) keeps starting from an
//! empty [`crate::agent::ToolRegistry`], so nothing outside this module and
//! `lib::service::mod`'s own tests ever depends on these tools existing.

mod bash;
mod fs;

use crate::agent::ToolRegistry;

/// Every built-in tool, registered under its stable name.
pub fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    fs::register(&mut registry);
    bash::register(&mut registry);
    registry
}

/// Runs a synchronous [`crate::vfs::Vfs`] call on a blocking thread — those
/// calls are ordinary (fast) syscalls, not async I/O, so they'd otherwise
/// block whichever executor thread happens to be driving the agent's turn.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result,
        Err(join_err) => Err(anyhow::anyhow!("tool task panicked: {join_err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_registers_every_built_in_tool_by_name() {
        let registry = registry();
        let mut names = registry.names();
        names.sort();
        assert_eq!(
            names,
            vec![
                "bash",
                "create_directory",
                "delete_file",
                "edit_file",
                "file_info",
                "list_directory",
                "read_file",
                "write_file",
            ]
        );
    }
}
