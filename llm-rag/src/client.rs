use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::net::UnixStream;

use crate::config::Config;
use crate::error::ClientError;
use crate::protocol::{Request, Response, framed, read_frame, write_frame};

const SERVER_SPAWN_BUDGET: Duration = Duration::from_secs(2);
const SERVER_SPAWN_POLL: Duration = Duration::from_millis(50);

pub async fn round_trip(
    cfg: &Config,
    socket: &Path,
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
    req: Request,
) -> Result<Response, ClientError> {
    let stream = connect_or_spawn(socket, config_override, secrets_override).await?;
    let mut fr = framed(stream);
    write_frame(&mut fr, &req).await?;
    let resp = tokio::time::timeout(
        Duration::from_secs(cfg.client_request_timeout_secs),
        read_frame::<_, Response>(&mut fr),
    )
    .await
    .map_err(|_| ClientError::RequestTimeout)??;
    Ok(resp)
}

/// Send a chat request and drive its streaming response, invoking `on_event`
/// for each frame (`ChatStart`, `ChatDelta`, `ChatToolCallDelta`, and either
/// `ChatDone` or `Response::Error` as the terminator). Applies
/// `client_stream_idle_timeout_secs` as a per-frame idle timeout so a silent
/// server eventually surfaces as `RequestTimeout` rather than hanging forever.
///
/// Returns `Ok(())` on either successful completion (`ChatDone`) or a
/// server-reported error (`Response::Error`) — callers inspect the frames to
/// distinguish. Returns `Err(StreamClosedMidResponse)` if the stream ends
/// before a terminal frame is seen.
pub async fn chat_stream<F>(
    cfg: &Config,
    socket: &Path,
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
    req: Request,
    mut on_event: F,
) -> Result<(), ClientError>
where
    F: FnMut(Response),
{
    debug_assert!(
        matches!(req, Request::Chat { .. }),
        "chat_stream only supports Request::Chat",
    );
    let stream = connect_or_spawn(socket, config_override, secrets_override).await?;
    let mut fr = framed(stream);
    write_frame(&mut fr, &req).await?;

    let idle = Duration::from_secs(cfg.client_stream_idle_timeout_secs);
    loop {
        let frame = match tokio::time::timeout(idle, read_frame::<_, Response>(&mut fr)).await {
            Ok(Ok(f)) => f,
            Ok(Err(crate::error::ProtocolError::StreamClosed)) => {
                return Err(ClientError::StreamClosedMidResponse);
            }
            Ok(Err(err)) => return Err(err.into()),
            Err(_) => return Err(ClientError::RequestTimeout),
        };
        let terminal = matches!(frame, Response::ChatDone { .. } | Response::Error { .. },);
        on_event(frame);
        if terminal {
            return Ok(());
        }
    }
}

async fn connect_or_spawn(
    socket: &Path,
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
) -> Result<UnixStream, ClientError> {
    match UnixStream::connect(socket).await {
        Ok(s) => return Ok(s),
        Err(err) if connect_should_spawn(&err) => {}
        Err(err) => return Err(err.into()),
    }
    spawn_detached_server(config_override, secrets_override).map_err(ClientError::SpawnServer)?;
    poll_connect(socket).await
}

fn connect_should_spawn(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::NotFound | ErrorKind::ConnectionRefused
    )
}

async fn poll_connect(socket: &Path) -> Result<UnixStream, ClientError> {
    let start = Instant::now();
    loop {
        match UnixStream::connect(socket).await {
            Ok(s) => return Ok(s),
            Err(err) if connect_should_spawn(&err) => {
                if start.elapsed() >= SERVER_SPAWN_BUDGET {
                    return Err(ClientError::ConnectTimeout {
                        socket: socket.to_path_buf(),
                    });
                }
                tokio::time::sleep(SERVER_SPAWN_POLL).await;
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn spawn_detached_server(
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
) -> Result<(), std::io::Error> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    if let Some(path) = config_override {
        cmd.arg("--config").arg(path);
    }
    if let Some(path) = secrets_override {
        cmd.arg("--secrets").arg(path);
    }
    cmd.arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}
