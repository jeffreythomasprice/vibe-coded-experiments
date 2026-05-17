//! Hierarchical document summarization pipeline.
//!
//! Builds a tree of summaries per document. Level 0 groups consecutive
//! `document_chunk` rows into summaries; level k > 0 groups level-(k-1)
//! summaries the same way. Each summary is also embedded so summary-aware
//! search can vector-rank summaries alongside chunks.
//!
//! Idempotent: a summary is identified within `(document_id, level)` by the
//! sha256 of its input texts. Re-running `summarize` on an unchanged document
//! skips every LLM call.
//!
//! Cancellable: each LLM call is wrapped in a cancel-watch select; on cancel,
//! returns `SummarizeError::Cancelled`. Any `document_summary` rows inserted
//! during this run are rolled back before returning, so cancellation leaves
//! the DB in the same state as if the run never started. A subsequent run
//! picks up via the content-hash skip.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch};
use turso::{Value, params::Params};

use crate::config::Config;
use crate::db::{Db, vector};
use crate::llm;
use crate::ollama;
use crate::protocol::{Event, ProgressEnvelope, ProgressEvent};

/// Hard cap on input bytes per LLM call. Sized to leave headroom above the
/// worst-case group at default settings (`chunk_size_bytes` 5000 × `group_size`
/// 6 × 1.5 grow-factor ≈ 45 KB) while staying well under the ~32K-token
/// context window of common local chat models (qwen2.5:14b, llama3.1:8b, …).
/// Errors out (rather than silently splitting) when exceeded, so the user can
/// shrink `summarize.group_size` or `chunking.chunk_size_bytes`.
const INPUT_BUDGET_BYTES: usize = 100_000;

const SYSTEM_PROMPT: &str = "\
You are summarizing a section of a longer document so a downstream search \
system can quickly judge whether the section is relevant to a user query. \
Write a single dense paragraph (no headings, no bullet points) that captures \
the concrete topics, entities, events, and claims found in the text. \
Prefer specific nouns over generalities. Do not editorialize or add framing \
like 'this section discusses'.";

#[derive(thiserror::Error, Debug)]
pub enum SummarizeError {
    #[error("canonicalizing path {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no document ingested at {path}")]
    DocumentNotFound { path: PathBuf },

    #[error("document at {path} has no chunks to summarize")]
    NoChunks { path: PathBuf },

    #[error(
        "summarize.group_size must be >= 2 (got {0}); a group of 1 would \
         just copy the input"
    )]
    InvalidGroupSize(usize),

    #[error(
        "input for level {level} group {group_index} is {bytes} bytes, over the \
         {budget}-byte LLM budget. Lower summarize.group_size or chunk_size_bytes."
    )]
    InputTooLarge {
        level: usize,
        group_index: usize,
        bytes: usize,
        budget: usize,
    },

    #[error("llm call: {0}")]
    Llm(#[from] crate::llm::LlmError),

    #[error("embedding summary: {0}")]
    Embed(#[from] crate::ollama::OllamaError),

    #[error("db error ({op}): {source}")]
    Db {
        op: &'static str,
        #[source]
        source: turso::Error,
    },

    #[error("summarize cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub struct SummarizeStats {
    pub new_summaries: u64,
    pub reused_summaries: u64,
    pub levels: usize,
}

/// A node at any level of the tree. At level 0 these come from
/// `document_chunk`; at higher levels they come from prior-level
/// `document_summary` rows. The fields below are everything a parent-level
/// build needs: stable id (for parent_id), byte/page span (to inherit into
/// the parent), and the text (to feed to the LLM and hash).
struct Node {
    id: i64,
    byte_start: i64,
    byte_end: i64,
    page_first: Option<i64>,
    page_last: Option<i64>,
    text: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn summarize(
    db: &Db,
    cfg: &Config,
    http: &reqwest::Client,
    raw_path: &Path,
    max_depth_override: Option<usize>,
    progress: Option<&mpsc::UnboundedSender<Event>>,
    cancel: Option<watch::Receiver<bool>>,
) -> Result<SummarizeStats, SummarizeError> {
    let mut inserted: Vec<i64> = Vec::new();
    let result = summarize_inner(
        db, cfg, http, raw_path, max_depth_override, progress, cancel, &mut inserted,
    )
    .await;
    if result.is_err() && !inserted.is_empty() {
        if let Err(e) = rollback_inserted(db, &inserted).await {
            // Best-effort cleanup; `queue cleanup` can repair later if this
            // fails for any reason.
            tracing::error!(
                error = %e,
                count = inserted.len(),
                "summarize: failed to clean up partial summaries after error",
            );
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn summarize_inner(
    db: &Db,
    cfg: &Config,
    http: &reqwest::Client,
    raw_path: &Path,
    max_depth_override: Option<usize>,
    progress: Option<&mpsc::UnboundedSender<Event>>,
    cancel: Option<watch::Receiver<bool>>,
    inserted: &mut Vec<i64>,
) -> Result<SummarizeStats, SummarizeError> {
    let group_size = cfg.summarize.group_size;
    if group_size < 2 {
        return Err(SummarizeError::InvalidGroupSize(group_size));
    }
    let max_depth = max_depth_override.unwrap_or(cfg.summarize.max_depth);

    let path = canonicalize(raw_path)?;
    let document_id = lookup_document_id(db, &path).await?;

    emit_stage(progress, "summarizing");

    let mut stats = SummarizeStats {
        new_summaries: 0,
        reused_summaries: 0,
        levels: 0,
    };

    // We have to load level 0 inputs to count chunks before we can estimate
    // total_levels; cheaper to just load them here and reuse them below.
    let level0_inputs = load_chunks_as_nodes(db, document_id).await?;
    if level0_inputs.is_empty() {
        return Err(SummarizeError::NoChunks { path: path.clone() });
    }
    let total_levels = estimate_total_levels(level0_inputs.len(), group_size, max_depth);
    let started_at = std::time::Instant::now();
    tracing::info!(
        path = %path.display(),
        chunk_count = level0_inputs.len(),
        group_size,
        max_depth,
        total_levels,
        "summarize: starting tree build",
    );

    let mut level: usize = 0;
    let mut level0_inputs = Some(level0_inputs);
    loop {
        let inputs = if level == 0 {
            level0_inputs.take().expect("level 0 inputs loaded above")
        } else {
            load_summaries_as_nodes(db, document_id, level - 1).await?
        };

        // Single-node level means the tree already has a root at level-1.
        if inputs.len() <= 1 {
            break;
        }

        if level >= max_depth {
            break;
        }

        let groups = group_consecutive(&inputs, group_size);

        // Track the new (parent) ids in order, parallel to `groups`. After
        // creating all level-k summaries we issue a single UPDATE for each
        // level-(k-1) summary to point at its parent. Chunks don't have
        // parent_id; that linkage is implicit via byte ranges.
        let mut parent_ids: Vec<i64> = Vec::with_capacity(groups.len());
        let total = groups.len();
        tracing::info!(
            path = %path.display(),
            level = level + 1,
            total_levels,
            groups = total,
            "summarize: starting level",
        );
        for (group_index, group) in groups.iter().enumerate() {
            check_cancel(cancel.as_ref())?;
            emit_progress(progress, level, total_levels, group_index + 1, total);

            let concat = concat_texts(group);
            if concat.len() > INPUT_BUDGET_BYTES {
                return Err(SummarizeError::InputTooLarge {
                    level,
                    group_index,
                    bytes: concat.len(),
                    budget: INPUT_BUDGET_BYTES,
                });
            }
            let hash = content_hash(&concat);

            let (summary_id, reused) =
                if let Some(id) = find_existing_summary(db, document_id, level, &hash).await? {
                    stats.reused_summaries += 1;
                    (id, true)
                } else {
                    let text = llm::complete(
                        http,
                        cfg,
                        SYSTEM_PROMPT,
                        &concat,
                        cancel.as_ref(),
                    )
                    .await?;
                    let embedding = ollama::embed(
                        http,
                        &cfg.ollama.url,
                        &cfg.ollama.embedding_model,
                        &text,
                        cancel.as_ref(),
                    )
                    .await?;

                    let (byte_start, byte_end, page_first, page_last) = span_of(group);
                    let id = insert_summary(
                        db,
                        document_id,
                        level as i64,
                        byte_start,
                        byte_end,
                        page_first,
                        page_last,
                        &text,
                        &hash,
                    )
                    .await?;
                    // Track for rollback BEFORE the vector insert: if that
                    // fails, we still want to clean up the parent row.
                    inserted.push(id);
                    insert_summary_vector(db, id, &embedding).await?;
                    stats.new_summaries += 1;
                    (id, false)
                };
            tracing::info!(
                level = level + 1,
                total_levels,
                group = group_index + 1,
                total_groups = total,
                reused,
                summary_id,
                elapsed_secs = started_at.elapsed().as_secs(),
                "summarize: group complete",
            );
            parent_ids.push(summary_id);
        }

        if level > 0 {
            // Wire each level-(k-1) summary up to its newly-created parent.
            for (group, parent_id) in groups.iter().zip(parent_ids.iter()) {
                for node in *group {
                    update_summary_parent(db, node.id, *parent_id).await?;
                }
            }
        }

        stats.levels = level + 1;
        level += 1;
    }

    Ok(stats)
}

/// Estimate how many summary levels the tree will end up with for a document
/// of `chunk_count` chunks, given `group_size` and `max_depth`. Returns at
/// least 1 so the formatter never divides by zero.
pub(crate) fn estimate_total_levels(
    chunk_count: usize,
    group_size: usize,
    max_depth: usize,
) -> usize {
    if chunk_count <= 1 || max_depth == 0 {
        return 1;
    }
    let g = group_size.max(2);
    let mut n = chunk_count;
    let mut levels = 0;
    while n > 1 && levels < max_depth {
        n = n.div_ceil(g);
        levels += 1;
    }
    levels.max(1)
}

fn canonicalize(p: &Path) -> Result<PathBuf, SummarizeError> {
    p.canonicalize().map_err(|source| SummarizeError::Canonicalize {
        path: p.to_path_buf(),
        source,
    })
}

async fn lookup_document_id(db: &Db, path: &Path) -> Result<i64, SummarizeError> {
    let path_str = path.to_string_lossy().to_string();
    let mut rows = db
        .conn
        .query("SELECT id FROM document WHERE path = ?", (path_str,))
        .await
        .map_err(|source| SummarizeError::Db {
            op: "lookup_document_id",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| SummarizeError::Db {
            op: "lookup_document_id.next",
            source,
        })?
        .ok_or_else(|| SummarizeError::DocumentNotFound {
            path: path.to_path_buf(),
        })?;
    row.get::<i64>(0).map_err(|source| SummarizeError::Db {
        op: "lookup_document_id.get",
        source,
    })
}

async fn load_chunks_as_nodes(db: &Db, document_id: i64) -> Result<Vec<Node>, SummarizeError> {
    let mut rows = db
        .conn
        .query(
            "SELECT id, byte_start, byte_end, page_first, page_last, text \
             FROM document_chunk WHERE document_id = ? ORDER BY byte_start ASC",
            (document_id,),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "load_chunks",
            source,
        })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| SummarizeError::Db {
        op: "load_chunks.next",
        source,
    })? {
        out.push(read_node_row(&row)?);
    }
    Ok(out)
}

async fn load_summaries_as_nodes(
    db: &Db,
    document_id: i64,
    level: usize,
) -> Result<Vec<Node>, SummarizeError> {
    let mut rows = db
        .conn
        .query(
            "SELECT id, byte_start, byte_end, page_first, page_last, text \
             FROM document_summary \
             WHERE document_id = ? AND level = ? ORDER BY byte_start ASC",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(level as i64),
            ]),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "load_summaries",
            source,
        })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| SummarizeError::Db {
        op: "load_summaries.next",
        source,
    })? {
        out.push(read_node_row(&row)?);
    }
    Ok(out)
}

fn read_node_row(row: &turso::Row) -> Result<Node, SummarizeError> {
    let id: i64 = row.get(0).map_err(|source| SummarizeError::Db {
        op: "node.get.id",
        source,
    })?;
    let byte_start: i64 = row.get(1).map_err(|source| SummarizeError::Db {
        op: "node.get.byte_start",
        source,
    })?;
    let byte_end: i64 = row.get(2).map_err(|source| SummarizeError::Db {
        op: "node.get.byte_end",
        source,
    })?;
    let page_first = match row.get_value(3) {
        Ok(Value::Null) => None,
        Ok(Value::Integer(n)) => Some(n),
        Ok(other) => panic!("unexpected page_first {other:?}"),
        Err(source) => {
            return Err(SummarizeError::Db {
                op: "node.get.page_first",
                source,
            });
        }
    };
    let page_last = match row.get_value(4) {
        Ok(Value::Null) => None,
        Ok(Value::Integer(n)) => Some(n),
        Ok(other) => panic!("unexpected page_last {other:?}"),
        Err(source) => {
            return Err(SummarizeError::Db {
                op: "node.get.page_last",
                source,
            });
        }
    };
    let text: String = row.get(5).map_err(|source| SummarizeError::Db {
        op: "node.get.text",
        source,
    })?;
    Ok(Node {
        id,
        byte_start,
        byte_end,
        page_first,
        page_last,
        text,
    })
}

fn group_consecutive<'a>(nodes: &'a [Node], group_size: usize) -> Vec<&'a [Node]> {
    nodes.chunks(group_size).collect()
}

fn concat_texts(group: &[Node]) -> String {
    // Double-newline separator gives the LLM a clean visual boundary between
    // adjacent chunks/summaries without merging into one continuous blob.
    let mut out = String::new();
    for (i, n) in group.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&n.text);
    }
    out
}

fn content_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn span_of(group: &[Node]) -> (i64, i64, Option<i64>, Option<i64>) {
    let byte_start = group.iter().map(|n| n.byte_start).min().expect("non-empty group");
    let byte_end = group.iter().map(|n| n.byte_end).max().expect("non-empty group");
    let page_first = group.iter().filter_map(|n| n.page_first).min();
    let page_last = group.iter().filter_map(|n| n.page_last).max();
    (byte_start, byte_end, page_first, page_last)
}

async fn find_existing_summary(
    db: &Db,
    document_id: i64,
    level: usize,
    hash: &str,
) -> Result<Option<i64>, SummarizeError> {
    let mut rows = db
        .conn
        .query(
            "SELECT id FROM document_summary \
             WHERE document_id = ? AND level = ? AND content_hash = ?",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(level as i64),
                Value::Text(hash.to_string()),
            ]),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "find_existing_summary",
            source,
        })?;
    match rows.next().await.map_err(|source| SummarizeError::Db {
        op: "find_existing_summary.next",
        source,
    })? {
        Some(row) => {
            let id: i64 = row.get(0).map_err(|source| SummarizeError::Db {
                op: "find_existing_summary.get",
                source,
            })?;
            Ok(Some(id))
        }
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_summary(
    db: &Db,
    document_id: i64,
    level: i64,
    byte_start: i64,
    byte_end: i64,
    page_first: Option<i64>,
    page_last: Option<i64>,
    text: &str,
    content_hash: &str,
) -> Result<i64, SummarizeError> {
    let pf = page_first.map(Value::Integer).unwrap_or(Value::Null);
    let pl = page_last.map(Value::Integer).unwrap_or(Value::Null);
    db.conn
        .execute(
            "INSERT INTO document_summary \
                (document_id, parent_id, level, byte_start, byte_end, \
                 page_first, page_last, text, content_hash) \
             VALUES (?, NULL, ?, ?, ?, ?, ?, ?, ?)",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(level),
                Value::Integer(byte_start),
                Value::Integer(byte_end),
                pf,
                pl,
                Value::Text(text.to_string()),
                Value::Text(content_hash.to_string()),
            ]),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "insert_summary",
            source,
        })?;
    let mut rows = db
        .conn
        .query("SELECT last_insert_rowid()", ())
        .await
        .map_err(|source| SummarizeError::Db {
            op: "insert_summary.last_id",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| SummarizeError::Db {
            op: "insert_summary.last_id.next",
            source,
        })?
        .expect("last_insert_rowid returns one row");
    row.get::<i64>(0).map_err(|source| SummarizeError::Db {
        op: "insert_summary.last_id.get",
        source,
    })
}

async fn insert_summary_vector(
    db: &Db,
    summary_id: i64,
    embedding: &[f32],
) -> Result<(), SummarizeError> {
    let blob = vector::to_blob(embedding);
    let sql = format!(
        "INSERT INTO {} (summary_id, embedding) VALUES (?, ?)",
        db.summary_vectors_table
    );
    db.conn
        .execute(
            &sql,
            Params::Positional(vec![Value::Integer(summary_id), Value::Blob(blob)]),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "insert_summary_vector",
            source,
        })?;
    Ok(())
}

async fn update_summary_parent(
    db: &Db,
    child_id: i64,
    parent_id: i64,
) -> Result<(), SummarizeError> {
    db.conn
        .execute(
            "UPDATE document_summary SET parent_id = ? WHERE id = ?",
            Params::Positional(vec![Value::Integer(parent_id), Value::Integer(child_id)]),
        )
        .await
        .map_err(|source| SummarizeError::Db {
            op: "update_summary_parent",
            source,
        })?;
    Ok(())
}

/// Undo summary rows inserted during this run.
///
/// Turso 0.4's `ON DELETE CASCADE` on `document_summary.parent_id` (a
/// self-reference) overflows the runtime stack on any delete against this
/// table, even for a single row with no children. We sidestep the cascade
/// entirely by turning off `foreign_keys` on a dedicated connection and
/// deleting dependents manually in the right order:
///   1. summary_vectors_<N> rows for these ids
///   2. detach children that were UPDATE'd to point at these ids
///   3. delete the rows themselves
async fn rollback_inserted(db: &Db, ids: &[i64]) -> Result<(), turso::Error> {
    let conn = db.database.connect()?;
    conn.execute("PRAGMA foreign_keys = OFF", ()).await?;

    let vec_table = db.summary_vectors_table.as_str();
    let del_vec_sql = format!("DELETE FROM {vec_table} WHERE summary_id = ?");

    for &id in ids {
        conn.execute(
            &del_vec_sql,
            Params::Positional(vec![Value::Integer(id)]),
        )
        .await?;
        conn.execute(
            "UPDATE document_summary SET parent_id = NULL WHERE parent_id = ?",
            Params::Positional(vec![Value::Integer(id)]),
        )
        .await?;
        conn.execute(
            "DELETE FROM document_summary WHERE id = ?",
            Params::Positional(vec![Value::Integer(id)]),
        )
        .await?;
    }
    Ok(())
}

fn check_cancel(rx: Option<&watch::Receiver<bool>>) -> Result<(), SummarizeError> {
    if let Some(rx) = rx {
        if *rx.borrow() {
            return Err(SummarizeError::Cancelled);
        }
    }
    Ok(())
}

fn emit_stage(progress: Option<&mpsc::UnboundedSender<Event>>, name: &str) {
    if let Some(tx) = progress {
        let _ = tx.send(Event::Progress(ProgressEnvelope {
            event: ProgressEvent::Stage {
                name: name.to_string(),
            },
            overall: None,
        }));
    }
}

fn emit_progress(
    progress: Option<&mpsc::UnboundedSender<Event>>,
    level: usize,
    total_levels: usize,
    current: usize,
    total: usize,
) {
    if let Some(tx) = progress {
        let _ = tx.send(Event::Progress(ProgressEnvelope {
            event: ProgressEvent::Summarizing {
                level,
                total_levels,
                current,
                total,
            },
            overall: None,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i64, byte_start: i64, byte_end: i64, text: &str) -> Node {
        Node {
            id,
            byte_start,
            byte_end,
            page_first: None,
            page_last: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn content_hash_is_stable() {
        let a = content_hash("hello world");
        let b = content_hash("hello world");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn content_hash_differs_for_different_input() {
        assert_ne!(content_hash("a"), content_hash("b"));
    }

    #[test]
    fn group_consecutive_partitions_evenly() {
        let nodes = vec![
            node(1, 0, 10, "a"),
            node(2, 10, 20, "b"),
            node(3, 20, 30, "c"),
            node(4, 30, 40, "d"),
            node(5, 40, 50, "e"),
        ];
        let groups = group_consecutive(&nodes, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1].len(), 2);
        assert_eq!(groups[2].len(), 1); // tail group of 1 is fine
    }

    #[test]
    fn span_of_aggregates_byte_and_page_ranges() {
        let group = vec![
            Node {
                id: 1,
                byte_start: 100,
                byte_end: 200,
                page_first: Some(2),
                page_last: Some(3),
                text: String::new(),
            },
            Node {
                id: 2,
                byte_start: 200,
                byte_end: 350,
                page_first: Some(3),
                page_last: Some(5),
                text: String::new(),
            },
        ];
        let (bs, be, pf, pl) = span_of(&group);
        assert_eq!(bs, 100);
        assert_eq!(be, 350);
        assert_eq!(pf, Some(2));
        assert_eq!(pl, Some(5));
    }

    #[test]
    fn span_of_handles_null_pages() {
        let group = vec![
            node(1, 0, 100, "a"),
            node(2, 100, 200, "b"),
        ];
        let (bs, be, pf, pl) = span_of(&group);
        assert_eq!(bs, 0);
        assert_eq!(be, 200);
        assert_eq!(pf, None);
        assert_eq!(pl, None);
    }

    #[test]
    fn concat_uses_double_newline_separator() {
        let group = vec![node(1, 0, 1, "alpha"), node(2, 1, 2, "beta")];
        assert_eq!(concat_texts(&group), "alpha\n\nbeta");
    }

    #[test]
    fn estimate_total_levels_geometric_decay() {
        // 77 chunks → 13 groups at level 0 → 3 at level 1 → 1 at level 2.
        // 3 levels of *summary work* (level 2 produces 1 summary).
        assert_eq!(estimate_total_levels(77, 6, 4), 3);
    }

    #[test]
    fn estimate_total_levels_clamps_to_max_depth() {
        // Very deep tree truncated by max_depth.
        assert_eq!(estimate_total_levels(10_000, 2, 3), 3);
    }

    #[test]
    fn estimate_total_levels_handles_tiny_inputs() {
        assert_eq!(estimate_total_levels(0, 6, 4), 1);
        assert_eq!(estimate_total_levels(1, 6, 4), 1);
        // 2 chunks → one group of 2 → one summary → one level.
        assert_eq!(estimate_total_levels(2, 6, 4), 1);
    }

    #[test]
    fn estimate_total_levels_degenerate_max_depth_zero() {
        assert_eq!(estimate_total_levels(100, 6, 0), 1);
    }
}
