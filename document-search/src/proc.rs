//! Running external programs: stdin closed, stdout/stderr captured, the child
//! killed if the job is cancelled. Shared by PDF extraction (`pdftotext`,
//! `pdftoppm`, `tesseract`), the OCR engine runners, and the `uv`/`git`
//! installer, which each map `ProcError` onto their own error types.

use std::process::{Output, Stdio};

use tokio::process::Command;
use tokio::sync::watch;

#[derive(Debug)]
pub enum ProcError {
    /// The program isn't on PATH (or the given path doesn't exist).
    NotFound,
    Spawn(std::io::Error),
    Wait(std::io::Error),
    /// The program ran and exited non-zero. Holds everything it printed.
    Failed(Output),
    Cancelled,
}

/// Run `cmd` to completion and return its output when it exits 0.
///
/// If `cancel` changes before the child exits (a cancel, or the sender being
/// dropped at server shutdown), the child is killed via `kill_on_drop` (the
/// dropped `wait_with_output` future owns it) and `Cancelled` is returned.
pub async fn run(
    mut cmd: Command,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Output, ProcError> {
    if cancel.is_some_and(|rx| *rx.borrow()) {
        return Err(ProcError::Cancelled);
    }
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProcError::NotFound
            } else {
                ProcError::Spawn(e)
            }
        })?;

    let wait_fut = child.wait_with_output();
    let output = match cancel {
        Some(rx) => {
            let mut rx = rx.clone();
            tokio::select! {
                res = wait_fut => res.map_err(ProcError::Wait)?,
                _ = rx.changed() => return Err(ProcError::Cancelled),
            }
        }
        None => wait_fut.await.map_err(ProcError::Wait)?,
    };

    if !output.status.success() {
        return Err(ProcError::Failed(output));
    }
    Ok(output)
}

/// The last `n` non-empty lines of a failed program's stderr (or stdout when
/// stderr is empty), for error messages. Python tracebacks and pip resolver
/// errors put the useful part at the end.
pub fn output_tail(output: &Output, n: usize) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        stderr
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(n)..];
    if tail.is_empty() {
        format!("exited with {}", output.status)
    } else {
        tail.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_program_is_not_found() {
        let err = run(Command::new("document-search-no-such-binary"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ProcError::NotFound), "{err:?}");
    }

    #[tokio::test]
    async fn failure_keeps_output_and_tail_takes_last_lines() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'a\\n\\nb\\nc\\n' >&2; exit 3"]);
        let ProcError::Failed(out) = run(cmd, None).await.unwrap_err() else {
            panic!("expected Failed");
        };
        assert_eq!(output_tail(&out, 2), "b\nc");
        assert_eq!(output_tail(&out, 10), "a\nb\nc");
    }

    #[tokio::test]
    async fn cancel_kills_the_child() {
        let (tx, rx) = watch::channel(false);
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let started = std::time::Instant::now();
        let fut = run(cmd, Some(&rx));
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = tx.send(true);
        });
        assert!(matches!(fut.await, Err(ProcError::Cancelled)));
        assert!(started.elapsed().as_secs() < 5);
    }
}
