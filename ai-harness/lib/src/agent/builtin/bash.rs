//! The `bash` tool: run a shell command confined by `lib::sandbox` to the
//! calling conversation's project mounts.
//!
//! Fails closed, always as an `is_error` tool result rather than a turn
//! failure: no working sandbox backend, or a project granting no
//! directories at all, is refused outright — this tool never falls back to
//! running unconfined. See `lib::sandbox`'s module doc for the mechanism;
//! this is the caller its own doc says is a "later addition".

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context};
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::agent::tool::{ctx_tool, JsonSchema, ToolContext, ToolOutput};
use crate::agent::ToolRegistry;
use crate::sandbox::SandboxSpec;
use crate::vfs::{normalize, Resolution, Vfs};

pub fn register(registry: &mut ToolRegistry) {
    registry.register(bash_tool());
}

#[derive(Debug, Deserialize, JsonSchema)]
struct BashArgs {
    /// The shell command to run, passed to `/bin/sh -c`.
    command: String,
    /// Absolute path, inside the project, to run the command in. Defaults
    /// to the project's first granted directory.
    #[serde(default)]
    cwd: Option<String>,
    /// Kill the command after this many seconds. Capped at this build's
    /// configured maximum regardless of what's asked for.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

fn bash_tool() -> crate::agent::FnTool {
    ctx_tool(
        "bash",
        "Run a shell command, confined to the project's own directories.",
        run_bash,
    )
    .with_system_prompt(
        "Runs `command` under `/bin/sh -c`, inside a sandbox that can only see this \
         conversation's project directories plus a minimal read-only OS image — nothing else \
         on the machine is reachable, and nothing this command does persists once it exits \
         except changes it makes inside a project directory that's read-write. Paths are real \
         absolute paths, same as the filesystem tools. Output is captured and, past a \
         configured size, truncated with an explicit marker — a command producing huge output \
         should be given a way to limit it (piping through `head`, for instance) rather than \
         relying on the truncation. A command that runs too long is killed and reported as \
         timed out. Requires the user's approval before it runs."
            .to_string(),
    )
    .requiring_approval()
}

async fn run_bash(ctx: ToolContext, args: BashArgs) -> anyhow::Result<ToolOutput> {
    if !ctx.sandbox_available() {
        bail!("bash is unavailable: no working sandbox backend is configured for this build");
    }
    let table = ctx.vfs().table().clone();
    if table.is_empty() {
        bail!("this conversation's project grants no directories; bash has nothing to run in");
    }

    let cwd = resolve_cwd(ctx.vfs(), args.cwd.as_deref())?;
    let limits = ctx.limits();
    let timeout_secs = args.timeout_secs.unwrap_or(limits.bash_timeout_secs).min(limits.bash_timeout_secs);

    let spec = SandboxSpec {
        table,
        program: OsString::from("/bin/sh"),
        args: vec![OsString::from("-c"), OsString::from(args.command)],
        cwd: Some(cwd),
        network: limits.network,
    };

    let std_command = ctx.sandbox().command(&spec)?;
    let mut command = tokio::process::Command::from(std_command);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().context("spawning the sandboxed shell")?;
    let stdout = child.stdout.take().expect("stdout was requested piped");
    let stderr = child.stderr.take().expect("stderr was requested piped");

    let cap = limits.max_output_bytes;
    let run = async {
        tokio::join!(drain_capped(stdout, cap), drain_capped(stderr, cap), child.wait())
    };

    let (stdout_result, stderr_result, status) = match tokio::time::timeout(Duration::from_secs(timeout_secs), run).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            bail!("command timed out after {timeout_secs}s and was killed");
        }
    };
    let status = status.context("waiting for the sandboxed shell to exit")?;

    Ok(ToolOutput::text(format_output(stdout_result, stderr_result, status, cap)))
}

/// `None` (from the model) means the project's first mount root — the
/// documented fallback `SandboxSpec.cwd` promises but `bwrap_argv` itself
/// does not implement (it only emits `--chdir` when `cwd` is `Some`); this
/// is the caller responsible for that promise. `Some(path)` is resolved
/// through the same `Vfs` every other tool uses and must land `Inside` the
/// project — never handed to the sandbox unvalidated.
fn resolve_cwd(vfs: &Vfs, cwd: Option<&str>) -> anyhow::Result<PathBuf> {
    match cwd {
        None => {
            let first = vfs
                .table()
                .mounts()
                .first()
                .expect("run_bash already checked table().is_empty()");
            Ok(first.root.clone())
        }
        Some(raw) => {
            let normalized = normalize(Path::new(raw)).with_context(|| format!("invalid cwd {raw:?}"))?;
            match vfs.table().resolve(&normalized) {
                Resolution::Inside { .. } => Ok(normalized),
                _ => bail!("cwd {raw:?} is outside every project directory"),
            }
        }
    }
}

/// Reads `reader` to EOF, but stops *keeping* bytes past `cap` — draining
/// rather than stopping outright so the child never blocks on a full pipe
/// while its output is discarded. This runs while the child is alive, not
/// after `wait_with_output` has already buffered everything: a command like
/// `yes` or `cat /dev/urandom` never gets the chance to exhaust memory
/// before the cap applies.
async fn drain_capped(mut reader: impl tokio::io::AsyncRead + Unpin, cap: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let take = (cap - buf.len()).min(n);
                    buf.extend_from_slice(&chunk[..take]);
                    if take < n {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

fn format_output(
    stdout: (Vec<u8>, bool),
    stderr: (Vec<u8>, bool),
    status: std::process::ExitStatus,
    cap: usize,
) -> String {
    let (out_bytes, out_truncated) = stdout;
    let (err_bytes, err_truncated) = stderr;

    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&out_bytes));
    if !err_bytes.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("--- stderr ---\n");
        text.push_str(&String::from_utf8_lossy(&err_bytes));
    }
    if out_truncated || err_truncated {
        text.push_str(&format!("\n[output truncated: showing at most {cap} bytes per stream]"));
    }
    let exit_desc = match status.code() {
        Some(code) => code.to_string(),
        None => "killed by signal".to_string(),
    };
    text.push_str(&format!("\n[exit status: {exit_desc}]"));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SandboxConfig;
    use crate::sandbox::{detect, SandboxBackend};
    use crate::vfs::{Mount, MountTable};
    use shared::project::AccessMode;
    use std::sync::Arc;

    /// `bwrap` is a free local binary — see `lib/tests/sandbox_bwrap.rs`,
    /// which this repo already runs unconditionally, not behind
    /// `AI_HARNESS_LIVE`. Skip (never fail) if this machine genuinely has no
    /// working sandbox, so a misconfigured environment reads as "skipped".
    fn sandbox() -> Option<Arc<dyn SandboxBackend>> {
        let (backend, availability) = detect(&SandboxConfig::default());
        if !availability.is_available() {
            eprintln!("skipping bash test: sandbox unavailable: {availability:?}");
            return None;
        }
        Some(backend)
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-harness-builtin-bash-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn ctx_with(sandbox: Arc<dyn SandboxBackend>, root: &Path) -> ToolContext {
        let table = MountTable::from_mounts(vec![Mount {
            root: root.to_path_buf(),
            mode: AccessMode::ReadWrite,
        }]);
        ToolContext::new(Vfs::new(table), sandbox, true, Default::default())
    }

    fn text_of(output: &ToolOutput) -> String {
        match &output.content[0] {
            shared::llm::ToolResultContent::Text { text } => text.clone(),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bash_is_registered_and_requires_approval() {
        let mut registry = ToolRegistry::new();
        register(&mut registry);
        let catalog = registry.catalog();
        let bash = catalog.iter().find(|t| t.def.name == "bash").unwrap();
        assert_eq!(bash.approval, shared::agent::Approval::RequiresApproval);
    }

    #[tokio::test]
    async fn an_unavailable_sandbox_is_an_is_error_result_not_an_unconfined_run() {
        let table = MountTable::from_mounts(vec![Mount {
            root: std::env::temp_dir(),
            mode: AccessMode::ReadWrite,
        }]);
        let ctx = ToolContext::new(
            Vfs::new(table),
            Arc::new(crate::sandbox::Disabled::new("test")),
            false,
            Default::default(),
        );
        let err = run_bash(
            ctx,
            BashArgs { command: "echo hi".to_string(), cwd: None, timeout_secs: None },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("no working sandbox"), "error was: {err}");
    }

    #[tokio::test]
    async fn an_empty_project_is_refused_even_with_a_working_sandbox() {
        let Some(sandbox) = sandbox() else { return };
        let ctx = ToolContext::new(Vfs::new(MountTable::default()), sandbox, true, Default::default());
        let err = run_bash(
            ctx,
            BashArgs { command: "echo hi".to_string(), cwd: None, timeout_secs: None },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("grants no directories"), "error was: {err}");
    }

    #[tokio::test]
    async fn a_successful_command_captures_its_stdout_and_exit_status() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("ok");
        let ctx = ctx_with(sandbox, &tmp.path);

        let output = run_bash(
            ctx,
            BashArgs { command: "echo hello".to_string(), cwd: None, timeout_secs: None },
        )
        .await
        .unwrap();
        let text = text_of(&output);
        assert!(text.contains("hello"), "output was: {text}");
        assert!(text.contains("[exit status: 0]"), "output was: {text}");
    }

    #[tokio::test]
    async fn a_command_can_only_see_its_own_project_directory() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("scoped");
        std::fs::write(tmp.path.join("visible.txt"), b"x").unwrap();
        let ctx = ctx_with(sandbox, &tmp.path);

        let output = run_bash(
            ctx,
            BashArgs { command: "ls /etc/shadow 2>&1 || true".to_string(), cwd: None, timeout_secs: None },
        )
        .await
        .unwrap();
        // /etc is not in system_paths by default in this test's mount table
        // (only the temp dir is mounted, and Disabled/real bwrap confines to
        // exactly the table plus its own system_paths) — a file outside the
        // project must not be readable regardless.
        let text = text_of(&output);
        assert!(
            text.contains("No such file") || text.contains("cannot") || text.contains("[exit status:"),
            "output was: {text}"
        );
    }

    #[tokio::test]
    async fn a_timed_out_command_is_killed_and_reported() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("timeout");
        let ctx = ctx_with(sandbox, &tmp.path);

        let err = run_bash(
            ctx,
            BashArgs { command: "sleep 30".to_string(), cwd: None, timeout_secs: Some(1) },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("timed out"), "error was: {err}");
    }

    #[tokio::test]
    async fn output_past_the_configured_cap_is_truncated_with_a_marker() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("cap");
        let table = MountTable::from_mounts(vec![Mount {
            root: tmp.path.clone(),
            mode: AccessMode::ReadWrite,
        }]);
        let ctx = ToolContext::new(
            Vfs::new(table),
            sandbox,
            true,
            crate::agent::ToolLimits {
                max_output_bytes: 16,
                ..Default::default()
            },
        );

        let output = run_bash(
            ctx,
            BashArgs {
                command: "printf 'a%.0s' $(seq 1 1000)".to_string(),
                cwd: None,
                timeout_secs: None,
            },
        )
        .await
        .unwrap();
        let text = text_of(&output);
        assert!(text.contains("[output truncated"), "output was: {text}");
    }

    #[tokio::test]
    async fn cwd_outside_the_project_is_refused() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("cwd");
        let ctx = ctx_with(sandbox, &tmp.path);

        let err = run_bash(
            ctx,
            BashArgs {
                command: "pwd".to_string(),
                cwd: Some("/etc".to_string()),
                timeout_secs: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("outside every project directory"), "error was: {err}");
    }

    #[tokio::test]
    async fn default_cwd_is_the_first_mount_root() {
        let Some(sandbox) = sandbox() else { return };
        let tmp = TempDir::new("default-cwd");
        let ctx = ctx_with(sandbox, &tmp.path);

        let output = run_bash(
            ctx,
            BashArgs { command: "pwd".to_string(), cwd: None, timeout_secs: None },
        )
        .await
        .unwrap();
        let text = text_of(&output);
        assert!(
            text.contains(tmp.path.to_str().unwrap()),
            "expected pwd to report {:?}, output was: {text}",
            tmp.path
        );
    }
}
