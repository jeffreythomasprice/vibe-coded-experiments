//! Server runtime: owns the DB + Ollama client, listens on a Unix socket,
//! serializes work through a single worker, and auto-exits when idle.

mod connection;

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::config::Config;
use crate::db::{self, Db};
use crate::protocol::{CurrentJob, Event, OutputMode, QueuedJob, Request, StatusSnapshot};
use crate::{commands, config as cfg_mod, ingest};

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("binding socket {path}: {source}")]
    Bind {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("db: {0}")]
    Db(#[from] crate::db::DbError),

    #[error("config: {0}")]
    Config(#[from] crate::config::ConfigError),
}

/// Shared mutable state. Always lock for short, sync sections — never across
/// `.await` points.
pub(crate) struct ServerState {
    pub started_at: Instant,
    pub queued: VecDeque<QueueEntry>,
    pub current: Option<RunningJob>,
    /// Ids of jobs that were dequeued by `queue delete`/`queue clear` while
    /// still sitting in the mpsc channel. When the worker pulls a Job with
    /// one of these ids it sends a Final and skips execution.
    pub cancelled_ids: HashSet<Uuid>,
    /// Shared so inline handlers (status, list, cancel) can read without
    /// going through the worker's queue. Holds the same `Db` the worker uses.
    pub db: Arc<Db>,
    /// Shared HTTP client for inline handlers that need to call Ollama
    /// (e.g. `search`). `reqwest::Client` is internally `Arc`, so cloning is
    /// cheap.
    pub http: reqwest::Client,
    /// Assembled config; inline handlers read `[search]`, `[ollama]`, etc.
    pub cfg: Arc<Config>,
}

pub(crate) struct QueueEntry {
    pub id: Uuid,
    pub label: String,
    pub enqueued_at: Instant,
    /// Held so `queue delete` can synthesize an `Event::Final` on the
    /// connection that submitted this job, before the worker ever sees it.
    pub event_tx: mpsc::UnboundedSender<Event>,
}

pub(crate) struct RunningJob {
    pub id: Uuid,
    pub label: String,
    pub started_at: Instant,
    /// Cooperative cancel signal. Held only when the current job supports
    /// cancellation; otherwise the receiver side is dropped immediately.
    pub cancel_tx: watch::Sender<bool>,
    /// Whether the running job polls `cancel_tx` and will stop on signal.
    /// Currently true for `Ingest` (which polls during chunk embedding and
    /// during its post-chunk summarize phase).
    pub is_cancellable: bool,
}

impl ServerState {
    pub(crate) fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            uptime_secs: self.started_at.elapsed().as_secs(),
            current: self.current.as_ref().map(|c| CurrentJob {
                id: c.id.to_string(),
                label: c.label.clone(),
                running_secs: c.started_at.elapsed().as_secs(),
            }),
            queued: self
                .queued
                .iter()
                .map(|q| QueuedJob {
                    id: q.id.to_string(),
                    label: q.label.clone(),
                    queued_secs: q.enqueued_at.elapsed().as_secs(),
                })
                .collect(),
        }
    }
}

pub(crate) struct Job {
    pub id: Uuid,
    pub req: Request,
    pub output_mode: OutputMode,
    pub event_tx: mpsc::UnboundedSender<Event>,
}

/// Run the server until idle-timeout or signal. The socket file is cleaned
/// up on the way out. If another server is already alive on the socket, this
/// returns `Ok(())` immediately so the auto-spawn path is a no-op.
pub async fn run(cfg: Config) -> Result<(), ServerError> {
    let socket_path = cfg.server.socket_path.clone();
    let idle_timeout = Duration::from_secs(cfg.server.idle_timeout_secs);

    if let Some(parent) = socket_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }

    let listener = match bind_or_take_over(&socket_path)? {
        Some(l) => l,
        None => {
            tracing::info!("server: another instance already alive, exiting");
            return Ok(());
        }
    };
    let _socket_guard = SocketGuard {
        path: socket_path.clone(),
    };

    tracing::info!(socket = %socket_path.display(), "server: listening");

    let http = reqwest::Client::new();
    let db = Arc::new(db::open(&cfg, &http).await?);
    match crate::cleanup::scan_and_repair(&db).await {
        Ok(report) if report.is_empty() => {}
        Ok(report) => tracing::info!(
            documents_repaired = report.documents_repaired,
            summary_rows_removed = report.summary_rows_removed,
            "server: startup cleanup repaired partial summaries",
        ),
        Err(e) => tracing::warn!(error = %e, "server: startup cleanup failed"),
    }
    let cfg_arc = Arc::new(cfg.clone());

    let state = Arc::new(Mutex::new(ServerState {
        started_at: Instant::now(),
        queued: VecDeque::new(),
        current: None,
        cancelled_ids: HashSet::new(),
        db: Arc::clone(&db),
        http: http.clone(),
        cfg: Arc::clone(&cfg_arc),
    }));

    let (job_tx, job_rx) = mpsc::channel::<Job>(32);
    let (client_done_tx, mut client_done_rx) = mpsc::channel::<()>(64);
    let (completed_tx, completed_rx) = watch::channel::<u64>(0);

    let worker = tokio::spawn(worker_loop(
        cfg.clone(),
        db,
        http,
        job_rx,
        Arc::clone(&state),
        completed_tx,
    ));

    let mut connected: u64 = 0;

    let shutdown = async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { tracing::info!("server: SIGINT"); }
            _ = wait_for_terminate() => { tracing::info!("server: SIGTERM"); }
        }
    };
    tokio::pin!(shutdown);

    loop {
        let idle = connected == 0
            && {
                let g = state.lock().unwrap();
                g.queued.is_empty() && g.current.is_none()
            };

        tokio::select! {
            biased;

            _ = &mut shutdown => {
                tracing::info!("server: shutting down on signal");
                break;
            }

            res = listener.accept() => {
                match res {
                    Ok((stream, _addr)) => {
                        connected += 1;
                        let state = Arc::clone(&state);
                        let job_tx = job_tx.clone();
                        let completed_rx = completed_rx.clone();
                        let client_done_tx = client_done_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = connection::handle(
                                stream,
                                state,
                                job_tx,
                                completed_rx,
                            ).await
                            {
                                tracing::warn!(error = %e, "server: connection error");
                            }
                            let _ = client_done_tx.send(()).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "server: accept failed");
                    }
                }
            }

            Some(_) = client_done_rx.recv() => {
                connected = connected.saturating_sub(1);
            }

            _ = sleep_if(idle, idle_timeout) => {
                tracing::info!("server: idle timeout, exiting");
                break;
            }
        }
    }

    // Stop accepting new connections.
    drop(listener);

    // Wait for any active connections to drain — they're either streaming
    // events for in-flight work or running an inline status request.
    while connected > 0 {
        match client_done_rx.recv().await {
            Some(_) => connected = connected.saturating_sub(1),
            None => break,
        }
    }

    // Worker exits when its job channel closes.
    drop(job_tx);
    let _ = worker.await;

    Ok(())
}

async fn sleep_if(idle: bool, dur: Duration) {
    if idle {
        tokio::time::sleep(dur).await;
    } else {
        std::future::pending::<()>().await;
    }
}

#[cfg(unix)]
async fn wait_for_terminate() {
    use tokio::signal::unix::{SignalKind, signal};
    if let Ok(mut s) = signal(SignalKind::terminate()) {
        s.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}

#[cfg(not(unix))]
async fn wait_for_terminate() {
    std::future::pending::<()>().await;
}

async fn worker_loop(
    cfg: Config,
    db: Arc<Db>,
    http: reqwest::Client,
    mut job_rx: mpsc::Receiver<Job>,
    state: Arc<Mutex<ServerState>>,
    completed_tx: watch::Sender<u64>,
) {
    let mut completed: u64 = 0;
    while let Some(job) = job_rx.recv().await {
        // Drop the queue entry for this id (which may be anywhere in the
        // queue — `queue delete` can remove a middle entry) and check whether
        // the job was cancelled while still queued.
        let was_cancelled = {
            let mut g = state.lock().unwrap();
            g.queued.retain(|q| q.id != job.id);
            g.cancelled_ids.remove(&job.id)
        };
        if was_cancelled {
            let _ = job.event_tx.send(Event::Final {
                ok: false,
                error: Some("removed from queue".to_string()),
            });
            completed += 1;
            let _ = completed_tx.send(completed);
            continue;
        }

        let label = job.req.label();
        let is_cancellable = matches!(job.req, Request::Ingest { .. });
        let (cancel_tx, cancel_rx) = watch::channel::<bool>(false);
        {
            let mut g = state.lock().unwrap();
            g.current = Some(RunningJob {
                id: job.id,
                label: label.clone(),
                started_at: Instant::now(),
                cancel_tx,
                is_cancellable,
            });
        }
        let _ = job.event_tx.send(Event::Started);

        let mode = job.output_mode;
        let result: Result<String, String> = match job.req {
            Request::Ingest { path, no_summary, max_depth } => {
                ingest::ingest(
                    &db,
                    &cfg,
                    &http,
                    &path,
                    no_summary,
                    max_depth,
                    Some(&job.event_tx),
                    Some(cancel_rx),
                )
                .await
                .map(|id| match mode {
                    OutputMode::Text => {
                        format!("ingested document id {id} from {}\n", path.display())
                    }
                    OutputMode::Json => serde_json::json!({
                        "ok": true,
                        "document_id": id,
                        "path": path.display().to_string(),
                    })
                    .to_string(),
                })
                .map_err(|e| e.to_string())
            }
            Request::Info { path } => match mode {
                OutputMode::Text => commands::info(&db, &path).await.map_err(|e| e.to_string()),
                OutputMode::Json => commands::info_json(&db, &path)
                    .await
                    .map_err(|e| e.to_string()),
            },
            Request::Text { path, range } => run_text(&db, &path, range, mode).await,
            Request::PrintConfig => match mode {
                OutputMode::Text => cfg_mod::render_toml(&cfg).map_err(|e| e.to_string()),
                OutputMode::Json => Ok(serde_json::json!({
                    "ok": true,
                    "config": &cfg,
                })
                .to_string()),
            },
            Request::Delete { path } => commands::delete(&db, &path)
                .await
                .map(|()| match mode {
                    OutputMode::Text => String::new(),
                    OutputMode::Json => serde_json::json!({
                        "ok": true,
                        "path": path.display().to_string(),
                    })
                    .to_string(),
                })
                .map_err(|e| e.to_string()),
            Request::TagAdd { path, tags } => match mode {
                OutputMode::Text => commands::tag_add(&db, &path, &tags)
                    .await
                    .map_err(|e| e.to_string()),
                OutputMode::Json => commands::tag_add_json(&db, &path, &tags)
                    .await
                    .map_err(|e| e.to_string()),
            },
            Request::TagRemove { path, tags } => match mode {
                OutputMode::Text => commands::tag_remove(&db, &path, &tags)
                    .await
                    .map_err(|e| e.to_string()),
                OutputMode::Json => commands::tag_remove_json(&db, &path, &tags)
                    .await
                    .map_err(|e| e.to_string()),
            },
            Request::Status { .. } => {
                unreachable!("status is handled inline by the connection task")
            }
            Request::QueueList
            | Request::QueueDelete { .. }
            | Request::QueueClear
            | Request::QueueCleanup => {
                unreachable!("queue commands are handled inline by the connection task")
            }
            Request::List { .. } => {
                unreachable!("list is handled inline by the connection task")
            }
            Request::TagList => {
                unreachable!("tag list is handled inline by the connection task")
            }
            Request::Cancel => unreachable!("cancel is handled inline by the connection task"),
            Request::Search { .. } => {
                unreachable!("search is handled inline by the connection task")
            }
        };

        match result {
            Ok(text) => {
                if !text.is_empty() {
                    let _ = job.event_tx.send(Event::Output { text });
                }
                let _ = job.event_tx.send(Event::Final {
                    ok: true,
                    error: None,
                });
            }
            Err(msg) => {
                let _ = job.event_tx.send(Event::Final {
                    ok: false,
                    error: Some(msg),
                });
            }
        }

        {
            let mut g = state.lock().unwrap();
            g.current = None;
        }
        completed += 1;
        let _ = completed_tx.send(completed);
    }
}

async fn run_text(
    db: &Db,
    path: &Path,
    range: crate::protocol::TextRangeReq,
    mode: OutputMode,
) -> Result<String, String> {
    use crate::protocol::TextRangeReq;
    let raw = match range.clone() {
        TextRangeReq::Bytes { start, end } => commands::text_bytes(db, path, start, end)
            .await
            .map_err(|e| e.to_string())?,
        TextRangeReq::Chars { start, end } => commands::text_chars(db, path, start, end)
            .await
            .map_err(|e| e.to_string())?,
        TextRangeReq::Pages { first, last } => commands::text_pages(db, path, first, last)
            .await
            .map_err(|e| e.to_string())?,
    };
    match mode {
        OutputMode::Text => Ok(raw),
        OutputMode::Json => Ok(commands::text_to_json(path, &range, &raw)),
    }
}

/// Bind the listener, recovering from a stale socket file. Returns `None` if
/// another server is already listening on this socket.
fn bind_or_take_over(path: &Path) -> Result<Option<UnixListener>, ServerError> {
    for attempt in 0..3 {
        match UnixListener::bind(path) {
            Ok(l) => return Ok(Some(l)),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                match std::os::unix::net::UnixStream::connect(path) {
                    Ok(_) => return Ok(None), // another server is alive
                    Err(_) => {
                        // Stale socket file: remove and retry. A concurrent
                        // peer may unlink + bind in this window, so we loop.
                        let _ = fs::remove_file(path);
                        if attempt == 2 {
                            return Err(ServerError::Bind {
                                path: path.to_path_buf(),
                                source: e,
                            });
                        }
                    }
                }
            }
            Err(e) => {
                return Err(ServerError::Bind {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
        }
    }
    Err(ServerError::Bind {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::Other,
            "exhausted bind retries",
        ),
    })
}

struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
