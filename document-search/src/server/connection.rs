//! Per-connection task: read one [`Request`], either handle it inline
//! (status) or enqueue a job and forward events back to the client.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::commands;
use crate::protocol::{Event, OutputMode, Request, RequestEnvelope};
use crate::server::{Job, QueueEntry, ServerState};

#[derive(thiserror::Error, Debug)]
pub enum ConnectionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("decoding request: {0}")]
    Decode(serde_json::Error),

    #[error("client closed before sending a request")]
    NoRequest,

    #[error("worker channel closed")]
    WorkerGone,
}

pub(crate) async fn handle(
    stream: UnixStream,
    state: Arc<Mutex<ServerState>>,
    job_tx: mpsc::Sender<Job>,
    completed_rx: watch::Receiver<u64>,
) -> Result<(), ConnectionError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(ConnectionError::NoRequest);
    }
    let envelope: RequestEnvelope =
        serde_json::from_str(line.trim_end()).map_err(ConnectionError::Decode)?;
    let output_mode = envelope.output_mode;
    let req = envelope.request;

    if let Request::Status { watch, interval_ms } = req {
        return handle_status(&mut write_half, &state, output_mode, watch, interval_ms).await;
    }

    if matches!(req, Request::QueueList) {
        let snapshot = state.lock().unwrap().snapshot();
        let text = match output_mode {
            OutputMode::Text => commands::queue_list_text(&snapshot),
            OutputMode::Json => commands::queue_list_json(&snapshot),
        };
        write_event(&mut write_half, &Event::Output { text }).await?;
        write_event(
            &mut write_half,
            &Event::Final { ok: true, error: None },
        )
        .await?;
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    if let Request::QueueDelete { id } = &req {
        return handle_queue_delete(&mut write_half, &state, output_mode, id).await;
    }

    if matches!(req, Request::QueueClear) {
        return handle_queue_clear(&mut write_half, &state, output_mode).await;
    }

    if matches!(req, Request::QueueCleanup) {
        let db = {
            let g = state.lock().unwrap();
            Arc::clone(&g.db)
        };
        let result = crate::cleanup::scan_and_repair(&db).await;
        match result {
            Ok(report) => {
                let text = match output_mode {
                    OutputMode::Text => crate::cleanup::render_text(&report),
                    OutputMode::Json => crate::cleanup::render_json(&report),
                };
                write_event(&mut write_half, &Event::Output { text }).await?;
                write_event(
                    &mut write_half,
                    &Event::Final { ok: true, error: None },
                )
                .await?;
            }
            Err(e) => {
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: false,
                        error: Some(e.to_string()),
                    },
                )
                .await?;
            }
        }
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    if let Request::List { tags, match_all } = &req {
        let (db, snapshot) = {
            let g = state.lock().unwrap();
            (Arc::clone(&g.db), g.snapshot())
        };
        let result = match output_mode {
            OutputMode::Text => commands::list_text(&db, &snapshot, tags, *match_all).await,
            OutputMode::Json => commands::list_json(&db, &snapshot, tags, *match_all).await,
        };
        match result {
            Ok(text) => {
                write_event(&mut write_half, &Event::Output { text }).await?;
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: true,
                        error: None,
                    },
                )
                .await?;
            }
            Err(e) => {
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: false,
                        error: Some(e.to_string()),
                    },
                )
                .await?;
            }
        }
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    if matches!(req, Request::TagList) {
        let db = {
            let g = state.lock().unwrap();
            Arc::clone(&g.db)
        };
        let result = match output_mode {
            OutputMode::Text => commands::tag_list_text(&db).await,
            OutputMode::Json => commands::tag_list_json(&db).await,
        };
        match result {
            Ok(text) => {
                write_event(&mut write_half, &Event::Output { text }).await?;
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: true,
                        error: None,
                    },
                )
                .await?;
            }
            Err(e) => {
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: false,
                        error: Some(e.to_string()),
                    },
                )
                .await?;
            }
        }
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    if let Request::Search {
        term,
        path,
        tags,
        match_all,
        limit,
        cutoff,
        no_truncate,
        include_summaries,
    } = &req
    {
        let (db, http, cfg) = {
            let g = state.lock().unwrap();
            (Arc::clone(&g.db), g.http.clone(), Arc::clone(&g.cfg))
        };
        let result = match output_mode {
            OutputMode::Text => {
                commands::search_text(
                    &db,
                    &http,
                    &cfg,
                    term,
                    path.as_deref(),
                    tags,
                    *match_all,
                    *limit,
                    *cutoff,
                    *no_truncate,
                    *include_summaries,
                )
                .await
            }
            OutputMode::Json => {
                commands::search_json(
                    &db,
                    &http,
                    &cfg,
                    term,
                    path.as_deref(),
                    tags,
                    *match_all,
                    *limit,
                    *cutoff,
                    *no_truncate,
                    *include_summaries,
                )
                .await
            }
        };
        match result {
            Ok(text) => {
                write_event(&mut write_half, &Event::Output { text }).await?;
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: true,
                        error: None,
                    },
                )
                .await?;
            }
            Err(e) => {
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: false,
                        error: Some(e.to_string()),
                    },
                )
                .await?;
            }
        }
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    if matches!(req, Request::Cancel) {
        let outcome: Result<String, String> = {
            let mut g = state.lock().unwrap();
            match g.current.as_mut() {
                Some(j) if j.is_cancellable => {
                    let _ = j.cancel_tx.send(true);
                    Ok(j.label.clone())
                }
                Some(j) => Err(format!(
                    "current job is not cancellable ({}); nothing to cancel",
                    j.label
                )),
                None => Err("no job is currently running".to_string()),
            }
        };
        match outcome {
            Ok(label) => {
                let text = match output_mode {
                    OutputMode::Text => format!("cancellation requested for: {label}\n"),
                    OutputMode::Json => {
                        serde_json::json!({"ok": true, "cancelled": label}).to_string()
                    }
                };
                write_event(&mut write_half, &Event::Output { text }).await?;
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: true,
                        error: None,
                    },
                )
                .await?;
            }
            Err(msg) => {
                write_event(
                    &mut write_half,
                    &Event::Final {
                        ok: false,
                        error: Some(msg),
                    },
                )
                .await?;
            }
        }
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    let label = req.label();
    let job_id = Uuid::new_v4();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();

    // Reserve a queue slot under the lock and capture the index this job
    // sits at relative to the worker's `completed` counter. `my_index` is
    // the value `completed` will hit just before this job starts running.
    let my_index = {
        let mut g = state.lock().unwrap();
        let baseline = *completed_rx.borrow();
        let idx = baseline + g.queued.len() as u64 + g.current.is_some() as u64;
        g.queued.push_back(QueueEntry {
            id: job_id,
            label: label.clone(),
            enqueued_at: Instant::now(),
            event_tx: event_tx.clone(),
        });
        idx
    };

    let initial_ahead = my_index.saturating_sub(*completed_rx.borrow());
    write_event(&mut write_half, &Event::QueuedAhead { ahead: initial_ahead }).await?;

    if job_tx
        .send(Job {
            id: job_id,
            req,
            output_mode,
            event_tx: event_tx.clone(),
        })
        .await
        .is_err()
    {
        // Worker is gone. Tell the client and bail.
        let _ = write_event(
            &mut write_half,
            &Event::Final {
                ok: false,
                error: Some("server is shutting down".to_string()),
            },
        )
        .await;
        return Err(ConnectionError::WorkerGone);
    }
    drop(event_tx);

    let mut watch_rx = completed_rx;
    let mut started = false;

    loop {
        tokio::select! {
            ev = event_rx.recv() => {
                match ev {
                    Some(e) => {
                        if matches!(e, Event::Started) { started = true; }
                        let is_final = matches!(e, Event::Final { .. });
                        write_event(&mut write_half, &e).await?;
                        if is_final { break; }
                    }
                    None => break,
                }
            }
            res = watch_rx.changed(), if !started => {
                if res.is_err() { break; }
                let completed = *watch_rx.borrow();
                let ahead = my_index.saturating_sub(completed);
                write_event(&mut write_half, &Event::QueuedAhead { ahead }).await?;
            }
        }
    }

    let _ = write_half.shutdown().await;
    Ok(())
}

async fn handle_status(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    state: &Arc<Mutex<ServerState>>,
    output_mode: OutputMode,
    watch: bool,
    interval_ms: u64,
) -> Result<(), ConnectionError> {
    if !watch {
        let snapshot = state.lock().unwrap().snapshot();
        match output_mode {
            OutputMode::Text => {
                write_event(write_half, &Event::StatusSnapshot(snapshot)).await?;
            }
            OutputMode::Json => {
                let text = commands::status_json(&snapshot);
                write_event(write_half, &Event::Output { text }).await?;
            }
        }
        write_event(
            write_half,
            &Event::Final { ok: true, error: None },
        )
        .await?;
        let _ = write_half.shutdown().await;
        return Ok(());
    }

    // Stream snapshots until the client disconnects (the write fails with
    // BrokenPipe). No Final is sent; the loop ends on peer close.
    let interval_ms = interval_ms.max(50);
    let mut tick = tokio::time::interval(Duration::from_millis(interval_ms));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let snapshot = state.lock().unwrap().snapshot();
        let result = match output_mode {
            OutputMode::Text => {
                write_event(write_half, &Event::StatusSnapshot(snapshot)).await
            }
            OutputMode::Json => {
                let text = commands::status_json(&snapshot);
                write_event(write_half, &Event::Output { text }).await
            }
        };
        if result.is_err() {
            // Peer closed (Ctrl-C). Exit cleanly without surfacing the error.
            break;
        }
    }
    Ok(())
}

/// Resolve a queue id from a full UUID string or an 8+ char hex prefix.
/// Returns (matched_uuid, target) where target says where it matched.
enum DeleteTarget {
    Running,
    Queued,
}

fn resolve_id(state: &ServerState, id: &str) -> Result<(Uuid, DeleteTarget), String> {
    let needle = id.trim().to_lowercase();
    if needle.is_empty() {
        return Err("id is empty".to_string());
    }
    let mut hits: Vec<(Uuid, DeleteTarget)> = Vec::new();
    if let Some(cur) = state.current.as_ref() {
        if cur.id.to_string().starts_with(&needle) {
            hits.push((cur.id, DeleteTarget::Running));
        }
    }
    for q in &state.queued {
        if q.id.to_string().starts_with(&needle) {
            hits.push((q.id, DeleteTarget::Queued));
        }
    }
    match hits.len() {
        0 => Err(format!("no queued or running job matches id {needle:?}")),
        1 => Ok(hits.into_iter().next().unwrap()),
        n => Err(format!("id {needle:?} is ambiguous ({n} matches)")),
    }
}

async fn handle_queue_delete(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    state: &Arc<Mutex<ServerState>>,
    output_mode: OutputMode,
    id: &str,
) -> Result<(), ConnectionError> {
    let outcome: Result<(Uuid, String, &'static str), String> = {
        let mut g = state.lock().unwrap();
        match resolve_id(&g, id) {
            Err(msg) => Err(msg),
            Ok((uuid, DeleteTarget::Running)) => {
                let cur = g.current.as_mut().expect("matched current");
                if !cur.is_cancellable {
                    Err(format!(
                        "current job is not cancellable ({}); nothing to cancel",
                        cur.label
                    ))
                } else {
                    let _ = cur.cancel_tx.send(true);
                    Ok((uuid, cur.label.clone(), "cancelling"))
                }
            }
            Ok((uuid, DeleteTarget::Queued)) => {
                // Remove the entry; notify the connection that submitted it;
                // mark id as cancelled so the worker skips when it pulls.
                let pos = g.queued.iter().position(|q| q.id == uuid).expect("matched queued");
                let entry = g.queued.remove(pos).expect("position exists");
                let label = entry.label.clone();
                let _ = entry.event_tx.send(Event::Final {
                    ok: false,
                    error: Some("removed from queue".to_string()),
                });
                g.cancelled_ids.insert(uuid);
                Ok((uuid, label, "removed"))
            }
        }
    };
    match outcome {
        Ok((uuid, label, action)) => {
            let text = match output_mode {
                OutputMode::Text => format!("{action} {uuid}: {label}\n"),
                OutputMode::Json => serde_json::json!({
                    "ok": true,
                    "action": action,
                    "id": uuid.to_string(),
                    "label": label,
                })
                .to_string(),
            };
            write_event(write_half, &Event::Output { text }).await?;
            write_event(
                write_half,
                &Event::Final { ok: true, error: None },
            )
            .await?;
        }
        Err(msg) => {
            write_event(
                write_half,
                &Event::Final {
                    ok: false,
                    error: Some(msg),
                },
            )
            .await?;
        }
    }
    let _ = write_half.shutdown().await;
    Ok(())
}

async fn handle_queue_clear(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    state: &Arc<Mutex<ServerState>>,
    output_mode: OutputMode,
) -> Result<(), ConnectionError> {
    let (cancelled_current, dropped_count, current_label) = {
        let mut g = state.lock().unwrap();
        let mut cancelled_current = false;
        let mut current_label = None;
        if let Some(cur) = g.current.as_mut() {
            if cur.is_cancellable {
                let _ = cur.cancel_tx.send(true);
                cancelled_current = true;
                current_label = Some(cur.label.clone());
            }
        }
        let dropped = g.queued.len();
        // Drain in one pass to avoid borrowing g twice.
        let entries: Vec<QueueEntry> = g.queued.drain(..).collect();
        for entry in entries {
            let _ = entry.event_tx.send(Event::Final {
                ok: false,
                error: Some("removed from queue".to_string()),
            });
            g.cancelled_ids.insert(entry.id);
        }
        (cancelled_current, dropped, current_label)
    };

    let text = match output_mode {
        OutputMode::Text => {
            let mut s = String::new();
            if cancelled_current {
                if let Some(label) = &current_label {
                    s.push_str(&format!("cancelling running job: {label}\n"));
                }
            }
            s.push_str(&format!("removed {dropped_count} queued job(s)\n"));
            s
        }
        OutputMode::Json => serde_json::json!({
            "ok": true,
            "cancelled_current": cancelled_current,
            "current_label": current_label,
            "dropped_queued": dropped_count,
        })
        .to_string(),
    };
    write_event(write_half, &Event::Output { text }).await?;
    write_event(
        write_half,
        &Event::Final { ok: true, error: None },
    )
    .await?;
    let _ = write_half.shutdown().await;
    Ok(())
}

async fn write_event(
    w: &mut tokio::net::unix::OwnedWriteHalf,
    ev: &Event,
) -> std::io::Result<()> {
    let mut s = serde_json::to_string(ev).expect("event serialization is infallible");
    s.push('\n');
    w.write_all(s.as_bytes()).await
}
