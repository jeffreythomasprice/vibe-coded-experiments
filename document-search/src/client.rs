//! Client mode: talk to the server over a Unix socket. Auto-spawns the
//! server if the socket is absent, then renders a spinner driven by the
//! event stream.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::config::Config;
use crate::protocol::{Event, ProgressEvent, Request, StatusSnapshot};

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("encoding request: {0}")]
    Encode(serde_json::Error),

    #[error("decoding event: {0}")]
    Decode(serde_json::Error),

    #[error("server closed before sending Final")]
    UnexpectedEof,

    #[error("server reported error: {0}")]
    Server(String),

    #[error("server failed to start (no socket at {path})")]
    SpawnTimeout { path: PathBuf },
}

/// Run a single request end-to-end. Returns `Ok(())` on a successful Final;
/// returns `Err(ClientError::Server)` if the server reported an error.
pub async fn run(cfg: &Config, req: Request, args_config: Option<&Path>) -> Result<(), ClientError> {
    let socket = &cfg.server.socket_path;
    let stream = connect_or_spawn(socket, args_config).await?;

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let mut line = serde_json::to_string(&req).map_err(ClientError::Encode)?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;
    write_half.flush().await?;

    render_events(&mut reader).await
}

async fn connect_or_spawn(
    sock: &Path,
    args_config: Option<&Path>,
) -> Result<UnixStream, ClientError> {
    if let Ok(s) = UnixStream::connect(sock).await {
        return Ok(s);
    }
    spawn_detached_server(args_config)?;

    // ~2s of retries with a small backoff.
    let mut delay = Duration::from_millis(50);
    for _ in 0..30 {
        tokio::time::sleep(delay).await;
        if let Ok(s) = UnixStream::connect(sock).await {
            return Ok(s);
        }
        delay = (delay * 2).min(Duration::from_millis(150));
    }

    Err(ClientError::SpawnTimeout {
        path: sock.to_path_buf(),
    })
}

fn spawn_detached_server(args_config: Option<&Path>) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    if let Some(c) = args_config {
        cmd.arg("--config").arg(c);
    }
    cmd.arg("server");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Detach from the current process group so killing the client doesn't
        // kill the server.
        unsafe {
            cmd.pre_exec(|| {
                if libc_setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    cmd.spawn()?;
    Ok(())
}

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "setsid"]
    fn libc_setsid() -> i32;
}

async fn render_events<R>(reader: &mut BufReader<R>) -> Result<(), ClientError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} {msg}")
            .expect("valid spinner template"),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Connecting...");

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            pb.finish_and_clear();
            return Err(ClientError::UnexpectedEof);
        }
        let ev: Event = serde_json::from_str(line.trim_end()).map_err(ClientError::Decode)?;
        match ev {
            Event::QueuedAhead { ahead } => {
                if ahead == 0 {
                    pb.set_message("Waiting to start...".to_string());
                } else if ahead == 1 {
                    pb.set_message("In queue, 1 task ahead".to_string());
                } else {
                    pb.set_message(format!("In queue, {ahead} tasks ahead"));
                }
            }
            Event::Started => {
                pb.set_message("Working...".to_string());
            }
            Event::Progress(ProgressEvent::Stage { name }) => {
                pb.set_message(name);
            }
            Event::Progress(ProgressEvent::Extracting { current, total }) => {
                pb.set_message(format!("Extracting page {current}/{total}"));
            }
            Event::Progress(ProgressEvent::Ocr { current, total }) => {
                pb.set_message(format!("OCR page {current}/{total}"));
            }
            Event::Progress(ProgressEvent::Chunking { current, total }) => {
                pb.set_message(format!("Chunking {current}/{total}"));
            }
            Event::Progress(ProgressEvent::Embedding { current, total }) => {
                pb.set_message(format!("Embedding chunk {current}/{total}"));
            }
            Event::Output { text } => {
                pb.suspend(|| print!("{text}"));
            }
            Event::StatusSnapshot(s) => {
                pb.finish_and_clear();
                print_status(&s);
            }
            Event::Final { ok, error } => {
                pb.finish_and_clear();
                if !ok {
                    return Err(ClientError::Server(error.unwrap_or_default()));
                }
                return Ok(());
            }
        }
    }
}

fn print_status(s: &StatusSnapshot) {
    println!("uptime: {}s", s.uptime_secs);
    match &s.current {
        Some(c) => println!("current: {} (running {}s)", c.label, c.running_secs),
        None => println!("current: idle"),
    }
    if s.queued.is_empty() {
        println!("queued: (empty)");
    } else {
        println!("queued ({}):", s.queued.len());
        for (i, q) in s.queued.iter().enumerate() {
            println!("  {}. {q}", i + 1);
        }
    }
}
