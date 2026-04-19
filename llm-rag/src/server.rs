use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;

use tracing::Instrument;

use crate::config::Config;
use crate::db::{self, EmbeddingModelInfo};
use crate::error::{CliError, ProtocolError, ServerStartError};
use crate::handlers::dispatch;
use crate::llm::{self, LlmStack};
use crate::protocol::{Request, framed, read_frame, write_frame};

/// Shared, read-through handles available to every connection. Wrapped in
/// `Arc` on the outside and cloned cheaply into each accept task.
pub struct ServerState {
    pub dal: Arc<db::Dal>,
    pub llm: Arc<LlmStack>,
}

pub async fn run(cfg: &Config, socket: &Path) -> Result<(), CliError> {
    ensure_parent_dir(socket)?;
    let listener = bind_with_stale_cleanup(socket).await?;
    tracing::info!(socket = %socket.display(), "llm-rag server listening");

    // Build shared state before accepting any connections: if these fail the
    // server dies cleanly with an appropriate CliError variant (and matching
    // exit code) rather than half-serving requests.
    let llm = llm::build(cfg)?;
    let model = EmbeddingModelInfo::from_config(cfg);
    tracing::info!(
        db_path = %cfg.db_path.display(),
        embedding_model = %model.name,
        dimensions = model.dimensions,
        "opening database",
    );
    let dal = db::open(cfg, model).await?;
    let state = Arc::new(ServerState {
        dal: Arc::new(dal),
        llm: Arc::new(llm),
    });

    let active = Arc::new(AtomicUsize::new(0));
    let activity = Arc::new(Notify::new());
    let idle = Duration::from_secs(cfg.server_idle_timeout_secs);

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _)) => {
                        active.fetch_add(1, Ordering::SeqCst);
                        let a = active.clone();
                        let n = activity.clone();
                        let s = state.clone();
                        let span = tracing::info_span!("conn");
                        tokio::spawn(
                            async move {
                                if let Err(err) = serve_connection(stream, s).await {
                                    tracing::error!(%err, "connection error");
                                }
                                a.fetch_sub(1, Ordering::SeqCst);
                                n.notify_one();
                            }
                            .instrument(span),
                        );
                    }
                    Err(err) => tracing::warn!(%err, "accept error"),
                }
            }
            _ = activity.notified() => {
                // Loop back so the idle-timer arm is re-evaluated with the current count.
            }
            _ = idle_sleep(&active, idle) => {
                tracing::info!(
                    idle_secs = cfg.server_idle_timeout_secs,
                    "idle timeout, shutting down"
                );
                break;
            }
        }
    }

    drop(listener);
    if let Err(err) = std::fs::remove_file(socket)
        && err.kind() != ErrorKind::NotFound
    {
        tracing::warn!(
            socket = %socket.display(),
            %err,
            "failed to remove socket"
        );
    }
    Ok(())
}

async fn idle_sleep(active: &AtomicUsize, idle: Duration) {
    if active.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(idle).await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn serve_connection(
    stream: UnixStream,
    state: Arc<ServerState>,
) -> Result<(), ProtocolError> {
    let mut fr = framed(stream);
    loop {
        let req = match read_frame::<_, Request>(&mut fr).await {
            Ok(req) => req,
            Err(ProtocolError::StreamClosed) => return Ok(()),
            Err(ProtocolError::Io(err))
                if matches!(
                    err.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe | ErrorKind::ConnectionReset
                ) =>
            {
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        let resp = dispatch(req, &state).await;
        write_frame(&mut fr, &resp).await?;
    }
}

fn ensure_parent_dir(socket: &Path) -> Result<(), ServerStartError> {
    if let Some(parent) = socket.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| ServerStartError::ParentDir {
            dir: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

async fn bind_with_stale_cleanup(socket: &Path) -> Result<UnixListener, ServerStartError> {
    match UnixListener::bind(socket) {
        Ok(l) => return Ok(l),
        Err(err) if err.kind() == ErrorKind::AddrInUse => match UnixStream::connect(socket).await {
            Ok(_) => {
                return Err(ServerStartError::AlreadyRunning {
                    socket: socket.to_path_buf(),
                });
            }
            Err(_) => {
                std::fs::remove_file(socket).map_err(|source| ServerStartError::RemoveStale {
                    socket: socket.to_path_buf(),
                    source,
                })?;
            }
        },
        Err(source) => {
            return Err(ServerStartError::Bind {
                socket: socket.to_path_buf(),
                source,
            });
        }
    }
    UnixListener::bind(socket).map_err(|source| ServerStartError::Bind {
        socket: socket.to_path_buf(),
        source,
    })
}
