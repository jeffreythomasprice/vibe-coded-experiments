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
use crate::progress_format::format_envelope;
use crate::protocol::{
    Event, OutputMode, ProgressEvent, Request, RequestEnvelope, StatusSnapshot,
};

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
pub async fn run(
    cfg: &Config,
    envelope: RequestEnvelope,
    args_config: Option<&Path>,
) -> Result<(), ClientError> {
    let socket = &cfg.server.socket_path;
    let stream = connect_or_spawn(socket, args_config).await?;

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let output_mode = envelope.output_mode;
    let watch_mode = matches!(envelope.request, Request::Status { watch: true, .. });
    let watch_interval_ms = match envelope.request {
        Request::Status { interval_ms, .. } => interval_ms,
        _ => 0,
    };
    let detach_path = match &envelope.request {
        Request::Ingest { path, detach: true, .. } => Some(path.clone()),
        _ => None,
    };
    let mut line = serde_json::to_string(&envelope).map_err(ClientError::Encode)?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;
    write_half.flush().await?;

    if let Some(path) = detach_path {
        return render_detach(&mut reader, output_mode, &path).await;
    }
    render_events(&mut reader, output_mode, watch_mode, watch_interval_ms).await
}

async fn render_detach<R>(
    reader: &mut BufReader<R>,
    output_mode: OutputMode,
    path: &Path,
) -> Result<(), ClientError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = String::new();
    let mut ahead: u64 = 0;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(ClientError::UnexpectedEof);
        }
        let ev: Event = serde_json::from_str(line.trim_end()).map_err(ClientError::Decode)?;
        match ev {
            Event::QueuedAhead { ahead: a } => ahead = a,
            Event::Final { ok, error } => {
                if !ok {
                    return Err(ClientError::Server(error.unwrap_or_default()));
                }
                match output_mode {
                    OutputMode::Text => {
                        let suffix = match ahead {
                            0 => "starting now".to_string(),
                            1 => "1 task ahead".to_string(),
                            n => format!("{n} tasks ahead"),
                        };
                        println!("queued ingest of {} ({suffix})", path.display());
                    }
                    OutputMode::Json => {
                        println!(
                            "{}",
                            serde_json::json!({
                                "ok": true,
                                "queued": true,
                                "path": path.display().to_string(),
                                "ahead": ahead,
                            })
                        );
                    }
                }
                return Ok(());
            }
            // Server isn't expected to emit these in detach mode, but be
            // defensive — just keep reading until Final.
            Event::Started | Event::Progress(_) | Event::Output { .. } | Event::StatusSnapshot(_) => {}
        }
    }
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

async fn render_events<R>(
    reader: &mut BufReader<R>,
    output_mode: OutputMode,
    watch_mode: bool,
    watch_interval_ms: u64,
) -> Result<(), ClientError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    match output_mode {
        OutputMode::Text => render_events_text(reader, watch_mode, watch_interval_ms).await,
        OutputMode::Json => render_events_json(reader).await,
    }
}

async fn render_events_text<R>(
    reader: &mut BufReader<R>,
    watch_mode: bool,
    watch_interval_ms: u64,
) -> Result<(), ClientError>
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
            Event::Progress(env) => {
                pb.set_message(format_envelope(&env));
            }
            Event::Output { text } => {
                pb.suspend(|| print!("{text}"));
            }
            Event::StatusSnapshot(s) => {
                pb.finish_and_clear();
                if watch_mode {
                    // Clear screen + move cursor home so the snapshot replaces
                    // the previous one in place.
                    print!("\x1b[2J\x1b[H");
                    print_status(&s);
                    println!(
                        "\n(refreshing every {watch_interval_ms}ms — Ctrl-C to exit)"
                    );
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                } else {
                    print_status(&s);
                }
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

/// JSON mode: stdout is exactly one JSON object per success; progress goes
/// to stderr as plain text. On a server error, print the error envelope to
/// stdout before returning so `main` doesn't double-print.
async fn render_events_json<R>(reader: &mut BufReader<R>) -> Result<(), ClientError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = String::new();
    let mut received_output = false;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(ClientError::UnexpectedEof);
        }
        let ev: Event = serde_json::from_str(line.trim_end()).map_err(ClientError::Decode)?;
        match ev {
            Event::QueuedAhead { ahead } => {
                if ahead == 0 {
                    eprintln!("waiting to start");
                } else {
                    eprintln!("queued: {ahead} ahead");
                }
            }
            Event::Started => {
                eprintln!("started");
            }
            Event::Progress(env) => {
                // Only surface stage transitions in JSON mode — per-chunk
                // counts would be too noisy on stderr.
                if let ProgressEvent::Stage { name } = env.event {
                    eprintln!("{name}");
                }
            }
            Event::Output { text } => {
                received_output = true;
                // Server already produced a JSON object. Strip any trailing
                // newline since `println!` will add one.
                let trimmed = text.trim_end_matches('\n');
                println!("{trimmed}");
            }
            Event::StatusSnapshot(s) => {
                // Server should send Output { text: json } for status in
                // JSON mode; this branch is defensive. Delegate to the
                // canonical formatter so the two paths can't drift.
                received_output = true;
                println!("{}", crate::commands::status_json(&s));
            }
            Event::Final { ok, error } => {
                if !ok {
                    let env = serde_json::json!({
                        "ok": false,
                        "error": error.clone().unwrap_or_default(),
                    });
                    println!("{env}");
                    return Err(ClientError::Server(error.unwrap_or_default()));
                }
                if !received_output {
                    // Defensive: every JSON-mode command should emit one
                    // Output, but synthesize a bare success envelope if not.
                    println!("{}", serde_json::json!({"ok": true}));
                }
                return Ok(());
            }
        }
    }
}

fn print_status(s: &StatusSnapshot) {
    println!("uptime: {}s", s.uptime_secs);
    match &s.current {
        Some(c) => {
            println!(
                "current: [{}] {} (running {}s)",
                short_id(&c.id),
                c.label,
                c.running_secs,
            );
            if let Some(env) = &c.progress {
                println!("         {}", format_envelope(env));
            }
        }
        None => println!("current: idle"),
    }
    if s.queued.is_empty() {
        println!("queued: (empty)");
    } else {
        println!("queued ({}):", s.queued.len());
        for (i, q) in s.queued.iter().enumerate() {
            println!(
                "  {}. [{}] {} (queued {}s)",
                i + 1,
                short_id(&q.id),
                q.label,
                q.queued_secs,
            );
        }
    }
}

fn short_id(id: &str) -> &str {
    // 8 hex chars is plenty to disambiguate jobs in the queue and matches
    // what `queue delete` accepts as a prefix.
    if id.len() >= 8 { &id[..8] } else { id }
}
