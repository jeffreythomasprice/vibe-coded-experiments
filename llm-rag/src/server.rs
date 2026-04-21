use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;
use tokio::sync::mpsc;

use tracing::Instrument;

use crate::config::{Config, EmbeddingsProviderConfig};
use crate::db::{self, DbError};
use crate::error::{CliError, ProtocolError, ServerStartError};
use crate::handlers::dispatch;
use crate::llm::{self, LlmStack, ollama::probe_ollama_dimensions};
use crate::protocol::{Request, Response, framed, read_frame, write_frame};
use crate::tools::ToolRegistry;

/// Shared, read-through handles available to every connection. Wrapped in
/// `Arc` on the outside and cloned cheaply into each accept task.
pub struct ServerState {
    pub dal: Arc<db::Dal>,
    pub llm: Arc<LlmStack>,
    pub tools: Arc<ToolRegistry>,
}

pub async fn run(cfg: &Config, socket: &Path) -> Result<(), CliError> {
    ensure_parent_dir(socket)?;
    let listener = bind_with_stale_cleanup(socket).await?;
    tracing::info!(socket = %socket.display(), "llm-rag server listening");

    // Once we've bound the socket, every exit path — happy or error — must
    // remove the file so a failed startup doesn't leave a zombie socket that
    // confuses the next client.
    let result = serve(cfg, listener).await;

    if let Err(err) = std::fs::remove_file(socket)
        && err.kind() != ErrorKind::NotFound
    {
        tracing::warn!(
            socket = %socket.display(),
            %err,
            "failed to remove socket"
        );
    }
    result
}

async fn serve(cfg: &Config, listener: UnixListener) -> Result<(), CliError> {
    // Build shared state before accepting any connections: if these fail the
    // server dies cleanly with an appropriate CliError variant (and matching
    // exit code) rather than half-serving requests.
    //
    // Order: build chat first (cheap, no I/O), then open the DB which resolves
    // the embedding model's vector length (cache hit, or one probe call), then
    // build the embeddings provider now that we know the dimensions.
    let chat = llm::build_chat(cfg)?;
    let (probe_url, probe_model) = match &cfg.llm.embeddings {
        EmbeddingsProviderConfig::Ollama { url, model } => (url.clone(), model.clone()),
    };
    tracing::info!(
        db_path = %cfg.db_path.display(),
        embedding_model = %probe_model,
        "opening database",
    );

    let probe_model_for_err = probe_model.clone();
    let probe_model_for_call = probe_model.clone();
    let dal = db::open(cfg, probe_model.clone(), move || async move {
        probe_ollama_dimensions(&probe_url, &probe_model_for_call)
            .await
            .map_err(|source| DbError::Probe {
                model: probe_model_for_err,
                source: source.into(),
            })
    })
    .await?;

    let embeddings = llm::build_embeddings(cfg, dal.model().dimensions)?;
    let state = Arc::new(ServerState {
        dal: Arc::new(dal),
        llm: Arc::new(LlmStack { chat, embeddings }),
        tools: Arc::new(ToolRegistry::with_defaults()),
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

    Ok(())
}

async fn idle_sleep(active: &AtomicUsize, idle: Duration) {
    if active.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(idle).await;
    } else {
        std::future::pending::<()>().await;
    }
}

/// Run a single request's dispatch and drop the sender so the writer loop
/// paired with this channel terminates. Errors from `dispatch` (only
/// `SendError` is possible — see handler docs) are silently absorbed: the
/// writer's error, if any, is the authoritative signal.
async fn run_dispatch(req: Request, state: &ServerState, tx: mpsc::Sender<Response>) {
    let _ = dispatch(req, state, &tx).await;
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

        // One channel per request. Dispatch emits frames into `tx`; we drain
        // them onto the wire concurrently. Both must finish before we read
        // the next request — otherwise frames from the next turn could race.
        //
        // If the client disconnects mid-stream the writer errors on send,
        // `rx` is dropped (at the end of its future), and dispatch's next
        // `tx.send(...)` fails — ending the in-flight handler without
        // persisting a partial turn (handlers only persist before `ChatDone`).
        let (tx, mut rx) = mpsc::channel::<Response>(32);
        let dispatch_fut = run_dispatch(req, &state, tx);
        let writer_fut = async {
            while let Some(resp) = rx.recv().await {
                write_frame(&mut fr, &resp).await?;
            }
            Ok::<(), ProtocolError>(())
        };
        let (_, writer_result) = tokio::join!(dispatch_fut, writer_fut);
        match writer_result {
            Ok(()) => {}
            Err(ProtocolError::Io(err))
                if matches!(
                    err.kind(),
                    ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof
                ) =>
            {
                return Ok(());
            }
            Err(err) => return Err(err),
        }
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
