//! Server runtime: owns the DB + Ollama client, listens on a Unix socket,
//! serializes work through a single worker, and auto-exits when idle.

mod connection;
mod tracker;

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
use crate::progress_format::format_envelope;
use crate::protocol::{
    CurrentJob, Event, OutputMode, ProgressEnvelope, ProgressEvent, QueuedJob, Request,
    StatusSnapshot,
};
use crate::{commands, config as cfg_mod, ingest};

pub(crate) use tracker::{JobTracker, SummaryPlan};

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
    /// Accumulates per-job progress: known phase totals, per-phase counters,
    /// and the most-recent envelope emitted to the client. The same relay
    /// task in `worker_loop` that forwards events to the client also feeds
    /// every event into this tracker, so the inline `status` handler renders
    /// the exact same envelope the foreground spinner saw.
    pub tracker: Arc<Mutex<JobTracker>>,
}

impl ServerState {
    pub(crate) fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            uptime_secs: self.started_at.elapsed().as_secs(),
            current: self.current.as_ref().map(|c| {
                let (progress, progress_age_secs) = c.tracker.lock().unwrap().latest();
                CurrentJob {
                    id: c.id.to_string(),
                    label: c.label.clone(),
                    running_secs: c.started_at.elapsed().as_secs(),
                    progress,
                    progress_age_secs,
                }
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
    if let Err(e) = db.task_log_repair_abandoned().await {
        tracing::warn!(error = %e, "server: task_log abandoned-row repair failed");
    }
    let state = Arc::new(Mutex::new(ServerState {
        started_at: Instant::now(),
        queued: VecDeque::new(),
        current: None,
        cancelled_ids: HashSet::new(),
        db: Arc::clone(&db),
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
        // Keep our own handle on the cancel state so we can distinguish a
        // cancelled-by-user failure from a real error after the handler returns.
        let cancel_observer = cancel_rx.clone();
        let job_started_at = Instant::now();
        // Ingests that will summarize get the tree's projected cost counted
        // from the start, so the overall %/ETA aren't wildly optimistic
        // until summarize begins.
        let summary_plan = match &job.req {
            Request::Ingest {
                no_summary: false,
                max_depth,
                summary_languages,
                ..
            } => Some(SummaryPlan {
                group_size: cfg.summarize.group_size,
                max_depth: max_depth.unwrap_or(cfg.summarize.max_depth),
                languages: crate::language::estimate_summary_language_count(
                    if summary_languages.is_empty() {
                        &cfg.summarize.languages
                    } else {
                        summary_languages
                    },
                ),
            }),
            _ => None,
        };
        let tracker: Arc<Mutex<JobTracker>> =
            Arc::new(Mutex::new(JobTracker::new(job_started_at, summary_plan)));
        {
            let mut g = state.lock().unwrap();
            g.current = Some(RunningJob {
                id: job.id,
                label: label.clone(),
                started_at: job_started_at,
                cancel_tx,
                is_cancellable,
                tracker: Arc::clone(&tracker),
            });
        }
        let _ = job.event_tx.send(Event::Started);

        let job_id_str = job.id.to_string();
        if let Some((task_name, path, tags)) = job.req.task_metrics() {
            if let Err(e) = db
                .task_log_start(&job_id_str, task_name, &label, path.as_deref(), tags.as_deref())
                .await
            {
                tracing::warn!(error = %e, job = %job_id_str, "task_log_start failed");
            }
        }

        let mode = job.output_mode;
        let result: Result<String, String> = match job.req {
            Request::Ingest {
                path,
                no_summary,
                max_depth,
                chunk_size,
                overlap,
                force_ocr,
                ocr_language,
                summary_languages,
                detach: _,
            } => {
                let opts = ingest::IngestOptions {
                    no_summary,
                    summary_max_depth: max_depth,
                    summary_languages,
                    chunk_size,
                    overlap,
                    extract: crate::pdf::ExtractOptions {
                        force_ocr,
                        language_override: ocr_language,
                    },
                };
                // Insert a relay between ingest and the connection. The
                // tracker accumulates known totals + per-phase counters,
                // then enriches each `ProgressEvent` with an `OverallProgress`
                // summary. The same enriched envelope is stored on the
                // tracker (so the inline `status` handler renders identical
                // wording) and forwarded to the connection. The relay keeps
                // feeding the tracker even after the client connection drops;
                // see `run_progress_relay` for the rationale.
                let (relay_tx, relay_rx) = mpsc::unbounded_channel::<Event>();
                let out_tx = job.event_tx.clone();
                let tracker_for_relay = Arc::clone(&tracker);
                let relay = tokio::spawn(run_progress_relay(
                    relay_rx,
                    out_tx,
                    tracker_for_relay,
                    label.clone(),
                ));
                let result = ingest::ingest(
                    &db,
                    &cfg,
                    &http,
                    &path,
                    &opts,
                    Some(&relay_tx),
                    Some(cancel_rx),
                )
                .await;
                // Let the relay drain and exit before we send `Final`, so a
                // heartbeat can't land on the wire after it.
                drop(relay_tx);
                let _ = relay.await;
                result.map(|id| match mode {
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
            Request::Search {
                term,
                path,
                tags,
                match_all,
                limit,
                cutoff,
                no_truncate,
                include_summaries,
            } => match mode {
                OutputMode::Text => commands::search_text(
                    &db,
                    &http,
                    &cfg,
                    &term,
                    path.as_deref(),
                    &tags,
                    match_all,
                    limit,
                    cutoff,
                    no_truncate,
                    include_summaries,
                )
                .await
                .map_err(|e| e.to_string()),
                OutputMode::Json => commands::search_json(
                    &db,
                    &http,
                    &cfg,
                    &term,
                    path.as_deref(),
                    &tags,
                    match_all,
                    limit,
                    cutoff,
                    no_truncate,
                    include_summaries,
                )
                .await
                .map_err(|e| e.to_string()),
            },
            Request::TaskLog { .. } => {
                unreachable!("task-log is handled inline by the connection task")
            }
        };

        let (log_status, log_error): (&str, Option<String>) = match &result {
            Ok(_) => ("success", None),
            Err(msg) => {
                let was_cancelled = *cancel_observer.borrow();
                if was_cancelled {
                    ("failure", Some("cancelled".to_string()))
                } else {
                    ("failure", Some(msg.clone()))
                }
            }
        };
        if let Err(e) = db
            .task_log_finish(&job_id_str, log_status, log_error.as_deref())
            .await
        {
            tracing::warn!(error = %e, job = %job_id_str, "task_log_finish failed");
        }

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

/// How often the relay re-sends the latest envelope with refreshed
/// elapsed / ETA / idle numbers while no new progress event has arrived.
const PROGRESS_HEARTBEAT: Duration = Duration::from_secs(1);
/// Maximum gap between progress log lines while a job is running.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(30);
/// Summary groups are slow (one LLM call each) so we log each one, but cap
/// the rate so a hash-reuse burst on re-ingest doesn't flood the log.
const SUMMARY_LOG_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Drain `relay_rx`, feed every `ProgressEvent` into the tracker, then
/// forward the enriched envelope to `out_tx`. Between events, re-send the
/// latest envelope every `PROGRESS_HEARTBEAT` so the foreground spinner's
/// elapsed / ETA / "last update" numbers stay live, matching `status`.
/// Also writes the same rendered line to the log (stage changes, each
/// summary group, and at least every `PROGRESS_LOG_INTERVAL`).
///
/// Keeps draining + observing even after `out_tx` is closed (i.e. the client
/// disconnected). Otherwise a detached ingest — or any case where the
/// foreground client exits before the ingest does — would freeze the tracker
/// on whichever event was current at disconnect time, so subsequent `status`
/// snapshots would all show the same stale event with an ever-growing "(Ns
/// ago)" suffix instead of the live progress that's still flowing.
///
/// Exits when `relay_rx` is closed (its sole sender — held by the worker
/// arm — drops when ingest returns).
async fn run_progress_relay(
    mut relay_rx: mpsc::UnboundedReceiver<Event>,
    out_tx: mpsc::UnboundedSender<Event>,
    tracker: Arc<Mutex<JobTracker>>,
    label: String,
) {
    let mut forward_alive = true;
    let mut heartbeat = tokio::time::interval(PROGRESS_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_log: Option<(Instant, String)> = None;
    loop {
        let out_ev = tokio::select! {
            ev = relay_rx.recv() => match ev {
                None => break,
                Some(Event::Progress(env_in)) => {
                    let envelope = tracker.lock().unwrap().record(env_in.event);
                    let phase = envelope.overall.as_ref().map(|o| o.phase.as_str()).unwrap_or("");
                    let should_log = match &last_log {
                        None => true,
                        Some((at, last_phase)) => {
                            matches!(envelope.event, ProgressEvent::Stage { .. })
                                || last_phase != phase
                                || at.elapsed() >= PROGRESS_LOG_INTERVAL
                                || (matches!(envelope.event, ProgressEvent::Summarizing { .. })
                                    && at.elapsed() >= SUMMARY_LOG_MIN_INTERVAL)
                        }
                    };
                    if should_log {
                        log_progress(&label, &envelope);
                        last_log = Some((Instant::now(), phase.to_string()));
                    }
                    heartbeat.reset();
                    Event::Progress(envelope)
                }
                Some(other) => other,
            },
            _ = heartbeat.tick() => {
                let (envelope, _) = tracker.lock().unwrap().latest();
                let Some(envelope) = envelope else { continue };
                if let Some((at, phase)) = &mut last_log
                    && at.elapsed() >= PROGRESS_LOG_INTERVAL
                {
                    log_progress(&label, &envelope);
                    *at = Instant::now();
                    if let Some(o) = &envelope.overall {
                        phase.clone_from(&o.phase);
                    }
                }
                Event::Progress(envelope)
            }
        };
        if forward_alive && out_tx.send(out_ev).is_err() {
            forward_alive = false;
        }
    }
}

fn log_progress(label: &str, env: &ProgressEnvelope) {
    let Some(o) = &env.overall else { return };
    let percent = if o.total_steps > 0 {
        Some((o.step * 100 / o.total_steps).min(100))
    } else {
        None
    };
    tracing::info!(
        job = %label,
        phase = %o.phase,
        step = o.step,
        total_steps = o.total_steps,
        percent = ?percent,
        elapsed_secs = o.elapsed_secs,
        eta_secs = ?o.eta_secs,
        idle_secs = ?o.idle_secs,
        "progress: {}",
        format_envelope(env),
    );
}

#[cfg(test)]
mod relay_tests {
    use super::*;
    use crate::protocol::{ProgressEnvelope, ProgressEvent};

    #[tokio::test]
    async fn relay_keeps_updating_tracker_after_client_disconnects() {
        let (relay_tx, relay_rx) = mpsc::unbounded_channel::<Event>();
        let (out_tx, out_rx) = mpsc::unbounded_channel::<Event>();
        let tracker = Arc::new(Mutex::new(JobTracker::new(Instant::now(), None)));

        let handle = tokio::spawn(run_progress_relay(
            relay_rx,
            out_tx,
            Arc::clone(&tracker),
            "test".to_string(),
        ));

        // Simulate the very first event a real ingest emits — the "extracting"
        // stage label. The relay should observe it and the client should
        // receive it.
        relay_tx
            .send(Event::Progress(ProgressEnvelope {
                event: ProgressEvent::Stage { name: "extracting".into() },
                overall: None,
            }))
            .unwrap();

        // Client disconnects.
        drop(out_rx);

        // Subsequent ingest events keep coming — pdftotext, then OCR.
        for current in 1..=5u32 {
            relay_tx
                .send(Event::Progress(ProgressEnvelope {
                    event: ProgressEvent::Extracting { current, total: 5 },
                    overall: None,
                }))
                .unwrap();
        }
        relay_tx
            .send(Event::Progress(ProgressEnvelope {
                event: ProgressEvent::Ocr { current: 9, total: 365 },
                overall: None,
            }))
            .unwrap();

        // Closing the sender lets the relay loop exit so we can assert.
        drop(relay_tx);
        handle.await.unwrap();

        let (latest, _age) = tracker.lock().unwrap().latest();
        let env = latest.expect("tracker should have a latest envelope");
        match env.event {
            ProgressEvent::Ocr { current: 9, total: 365 } => {}
            other => panic!(
                "expected tracker latest to be the final OCR event \
                 (post-disconnect drain), got {other:?}",
            ),
        }
    }
}
