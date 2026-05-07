//! Per-connection task: read one [`Request`], either handle it inline
//! (status) or enqueue a job and forward events back to the client.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};

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

    if matches!(req, Request::Status) {
        let snapshot = state.lock().unwrap().snapshot();
        match output_mode {
            OutputMode::Text => {
                write_event(&mut write_half, &Event::StatusSnapshot(snapshot)).await?;
            }
            OutputMode::Json => {
                let text = commands::status_json(&snapshot);
                write_event(&mut write_half, &Event::Output { text }).await?;
            }
        }
        write_event(
            &mut write_half,
            &Event::Final {
                ok: true,
                error: None,
            },
        )
        .await?;
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
                Some(j) if j.is_ingest => {
                    let _ = j.cancel_tx.send(true);
                    Ok(j.label.clone())
                }
                Some(j) => Err(format!(
                    "current job is not an ingest ({}); nothing to cancel",
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
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();

    // Reserve a queue slot under the lock and capture the index this job
    // sits at relative to the worker's `completed` counter. `my_index` is
    // the value `completed` will hit just before this job starts running.
    let my_index = {
        let mut g = state.lock().unwrap();
        let baseline = *completed_rx.borrow();
        let idx = baseline + g.queued.len() as u64 + g.current.is_some() as u64;
        g.queued.push_back(QueueEntry {
            label: label.clone(),
        });
        idx
    };

    let initial_ahead = my_index.saturating_sub(*completed_rx.borrow());
    write_event(&mut write_half, &Event::QueuedAhead { ahead: initial_ahead }).await?;

    if job_tx
        .send(Job {
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

async fn write_event(
    w: &mut tokio::net::unix::OwnedWriteHalf,
    ev: &Event,
) -> std::io::Result<()> {
    let mut s = serde_json::to_string(ev).expect("event serialization is infallible");
    s.push('\n');
    w.write_all(s.as_bytes()).await
}
