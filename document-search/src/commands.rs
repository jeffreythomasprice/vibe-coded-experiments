//! Read-side CLI command handlers: `info` and the three flavors of `text`.

use std::cmp::Ordering;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use turso::{Value, params::Params};

use crate::config::Config;
use crate::db::{Db, vector};
use crate::ollama;
use crate::protocol::{StatusSnapshot, TextRangeReq};

#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error("canonicalizing path {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no document ingested at {path}")]
    DocumentNotFound { path: PathBuf },

    #[error("document {path} has no pages (not a PDF)")]
    NotPdf { path: PathBuf },

    #[error("byte range {start}..={end} out of bounds for size {total}")]
    ByteRangeOutOfBounds { start: u64, end: u64, total: u64 },

    #[error("char range {start}..={end} out of bounds for size {total}")]
    CharRangeOutOfBounds { start: u64, end: u64, total: u64 },

    #[error("page range {first}..={last} out of bounds for total pages {total}")]
    PageRangeOutOfBounds { first: u32, last: u32, total: u32 },

    #[error("range start ({start}) is greater than end ({end})")]
    InvalidRange { start: u64, end: u64 },

    #[error("page range first ({first}) is greater than last ({last})")]
    InvalidPageRange { first: u32, last: u32 },

    #[error(
        "stored chunks do not cover byte {missing}; this should be impossible — the DB may be corrupted"
    )]
    ChunkCoverageGap { missing: u64 },

    #[error("db error during command ({op}): {source}")]
    Db {
        op: &'static str,
        #[source]
        source: turso::Error,
    },

    #[error("opening read connection: {source}")]
    FreshConn {
        #[source]
        source: crate::db::DbError,
    },

    #[error("empty tag (after trimming whitespace) is not allowed")]
    EmptyTag,

    #[error("must specify --path or at least one --tag for search")]
    SearchScopeRequired,

    #[error("embedding query term: {source}")]
    EmbedQuery {
        #[source]
        source: crate::ollama::OllamaError,
    },

    #[error("query embedding has {got} dims, expected {expected} for model {model:?}")]
    EmbedDimMismatch {
        got: usize,
        expected: usize,
        model: String,
    },

    #[error("invalid --limit: must be >= 1")]
    InvalidLimit,

    #[error("invalid --cutoff {0}: must be in [0.0, 1.0]")]
    InvalidCutoff(f32),
}

struct DocumentRow {
    id: i64,
    doc_type: String,
    total_size_bytes: i64,
    total_size_chars: i64,
    total_size_pages: Option<i64>,
}

pub async fn info(db: &Db, raw_path: &Path) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;

    let mut out = String::new();
    let _ = writeln!(out, "path:             {}", path.display());
    let _ = writeln!(out, "type:             {}", row.doc_type);
    let _ = writeln!(out, "total_size_bytes: {}", row.total_size_bytes);
    let _ = writeln!(out, "total_size_chars: {}", row.total_size_chars);
    match row.total_size_pages {
        Some(p) => {
            let _ = writeln!(out, "total_size_pages: {p}");
        }
        None => {
            let _ = writeln!(out, "total_size_pages: null");
        }
    }

    let tags = fetch_document_tags(db, row.id).await?;
    if tags.is_empty() {
        let _ = writeln!(out, "tags:             (none)");
    } else {
        let _ = writeln!(out, "tags:             {}", tags.join(", "));
    }
    Ok(out)
}

pub async fn info_json(db: &Db, raw_path: &Path) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let tags = fetch_document_tags(db, row.id).await?;
    let env = serde_json::json!({
        "ok": true,
        "path": path.display().to_string(),
        "type": row.doc_type,
        "total_size_bytes": row.total_size_bytes,
        "total_size_chars": row.total_size_chars,
        "total_size_pages": row.total_size_pages,
        "tags": tags,
    });
    Ok(env.to_string())
}

async fn fetch_document_tags(db: &Db, document_id: i64) -> Result<Vec<String>, CommandError> {
    let mut rows = db
        .conn
        .query(
            "SELECT tag FROM document_tag WHERE document_id = ? ORDER BY tag",
            (document_id,),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "fetch_document_tags",
            source,
        })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "fetch_document_tags.next",
        source,
    })? {
        let t: String = row.get(0).map_err(|source| CommandError::Db {
            op: "fetch_document_tags.get",
            source,
        })?;
        out.push(t);
    }
    Ok(out)
}

fn normalize_tags(raw: &[String]) -> Result<Vec<String>, CommandError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for t in raw {
        let n = t.trim().to_lowercase();
        if n.is_empty() {
            return Err(CommandError::EmptyTag);
        }
        if seen.insert(n.clone()) {
            out.push(n);
        }
    }
    Ok(out)
}

pub async fn tag_add(
    db: &Db,
    raw_path: &Path,
    raw_tags: &[String],
) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let tags = normalize_tags(raw_tags)?;

    // Determine added vs already-present counts up front. turso 0.4.4's
    // execute() return value over-counts DELETEs on tables with secondary
    // indexes, so we compute mutation counts in Rust and treat the SQL writes
    // as fire-and-forget on success.
    let existing: std::collections::BTreeSet<String> =
        fetch_document_tags(db, row.id).await?.into_iter().collect();
    let to_insert: Vec<&String> = tags.iter().filter(|t| !existing.contains(*t)).collect();
    let already = (tags.len() - to_insert.len()) as u64;
    let added = to_insert.len() as u64;

    db.conn
        .execute("BEGIN", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_add.begin",
            source,
        })?;

    for tag in &to_insert {
        if let Err(source) = db
            .conn
            .execute(
                "INSERT OR IGNORE INTO document_tag (document_id, tag) VALUES (?, ?)",
                Params::Positional(vec![Value::Integer(row.id), Value::Text((*tag).clone())]),
            )
            .await
        {
            let _ = db.conn.execute("ROLLBACK", ()).await;
            return Err(CommandError::Db {
                op: "tag_add.insert",
                source,
            });
        }
    }

    db.conn
        .execute("COMMIT", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_add.commit",
            source,
        })?;

    Ok(format!(
        "added {added} tag(s) to {}; {already} already present\n",
        path.display()
    ))
}

pub async fn tag_add_json(
    db: &Db,
    raw_path: &Path,
    raw_tags: &[String],
) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let tags = normalize_tags(raw_tags)?;

    let existing: std::collections::BTreeSet<String> =
        fetch_document_tags(db, row.id).await?.into_iter().collect();
    let to_insert: Vec<&String> = tags.iter().filter(|t| !existing.contains(*t)).collect();
    let already = (tags.len() - to_insert.len()) as u64;
    let added = to_insert.len() as u64;

    db.conn
        .execute("BEGIN", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_add_json.begin",
            source,
        })?;

    for tag in &to_insert {
        if let Err(source) = db
            .conn
            .execute(
                "INSERT OR IGNORE INTO document_tag (document_id, tag) VALUES (?, ?)",
                Params::Positional(vec![Value::Integer(row.id), Value::Text((*tag).clone())]),
            )
            .await
        {
            let _ = db.conn.execute("ROLLBACK", ()).await;
            return Err(CommandError::Db {
                op: "tag_add_json.insert",
                source,
            });
        }
    }

    db.conn
        .execute("COMMIT", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_add_json.commit",
            source,
        })?;

    Ok(serde_json::json!({
        "ok": true,
        "path": path.display().to_string(),
        "added": added,
        "already_present": already,
    })
    .to_string())
}

pub async fn tag_remove(
    db: &Db,
    raw_path: &Path,
    raw_tags: &[String],
) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let tags = normalize_tags(raw_tags)?;

    let existing: std::collections::BTreeSet<String> =
        fetch_document_tags(db, row.id).await?.into_iter().collect();
    let to_remove: Vec<&String> = tags.iter().filter(|t| existing.contains(*t)).collect();
    let removed = to_remove.len() as u64;

    db.conn
        .execute("BEGIN", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_remove.begin",
            source,
        })?;

    for tag in &to_remove {
        if let Err(source) = db
            .conn
            .execute(
                "DELETE FROM document_tag WHERE document_id = ? AND tag = ?",
                Params::Positional(vec![Value::Integer(row.id), Value::Text((*tag).clone())]),
            )
            .await
        {
            let _ = db.conn.execute("ROLLBACK", ()).await;
            return Err(CommandError::Db {
                op: "tag_remove.delete",
                source,
            });
        }
    }

    db.conn
        .execute("COMMIT", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_remove.commit",
            source,
        })?;

    Ok(format!(
        "removed {removed} tag(s) from {}\n",
        path.display()
    ))
}

pub async fn tag_remove_json(
    db: &Db,
    raw_path: &Path,
    raw_tags: &[String],
) -> Result<String, CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let tags = normalize_tags(raw_tags)?;

    let existing: std::collections::BTreeSet<String> =
        fetch_document_tags(db, row.id).await?.into_iter().collect();
    let to_remove: Vec<&String> = tags.iter().filter(|t| existing.contains(*t)).collect();
    let removed = to_remove.len() as u64;

    db.conn
        .execute("BEGIN", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_remove_json.begin",
            source,
        })?;

    for tag in &to_remove {
        if let Err(source) = db
            .conn
            .execute(
                "DELETE FROM document_tag WHERE document_id = ? AND tag = ?",
                Params::Positional(vec![Value::Integer(row.id), Value::Text((*tag).clone())]),
            )
            .await
        {
            let _ = db.conn.execute("ROLLBACK", ()).await;
            return Err(CommandError::Db {
                op: "tag_remove_json.delete",
                source,
            });
        }
    }

    db.conn
        .execute("COMMIT", ())
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_remove_json.commit",
            source,
        })?;

    Ok(serde_json::json!({
        "ok": true,
        "path": path.display().to_string(),
        "removed": removed,
    })
    .to_string())
}

pub async fn tag_list_text(db: &Db) -> Result<String, CommandError> {
    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;

    let mut rows = conn
        .query(
            "SELECT tag, COUNT(*) AS n FROM document_tag GROUP BY tag ORDER BY tag",
            (),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_list_text.query",
            source,
        })?;

    let mut entries: Vec<(String, i64)> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "tag_list_text.next",
        source,
    })? {
        let tag: String = row.get(0).map_err(|source| CommandError::Db {
            op: "tag_list_text.get.tag",
            source,
        })?;
        let n: i64 = row.get(1).map_err(|source| CommandError::Db {
            op: "tag_list_text.get.n",
            source,
        })?;
        entries.push((tag, n));
    }

    let mut out = String::new();
    let _ = writeln!(out, "tags ({}):", entries.len());
    if entries.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        let tag_w = entries.iter().map(|(t, _)| t.len()).max().unwrap_or(0);
        let count_w = entries
            .iter()
            .map(|(_, n)| n.to_string().len())
            .max()
            .unwrap_or(0);
        for (tag, n) in &entries {
            let _ = writeln!(
                out,
                "  {tag:<tag_w$}  {n:>count_w$}",
                tag = tag,
                n = n,
                tag_w = tag_w,
                count_w = count_w,
            );
        }
    }
    Ok(out)
}

pub async fn tag_list_json(db: &Db) -> Result<String, CommandError> {
    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;

    let mut rows = conn
        .query(
            "SELECT tag, COUNT(*) AS n FROM document_tag GROUP BY tag ORDER BY tag",
            (),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "tag_list_json.query",
            source,
        })?;

    let mut entries: Vec<serde_json::Value> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "tag_list_json.next",
        source,
    })? {
        let tag: String = row.get(0).map_err(|source| CommandError::Db {
            op: "tag_list_json.get.tag",
            source,
        })?;
        let n: i64 = row.get(1).map_err(|source| CommandError::Db {
            op: "tag_list_json.get.n",
            source,
        })?;
        entries.push(serde_json::json!({"tag": tag, "count": n}));
    }

    Ok(serde_json::json!({"ok": true, "tags": entries}).to_string())
}

pub async fn delete(db: &Db, raw_path: &Path) -> Result<(), CommandError> {
    let path = canonicalize(raw_path)?;
    // Surfaces DocumentNotFound when the path isn't ingested, instead of a
    // silent no-op DELETE.
    let _ = lookup_document(db, &path).await?;

    let path_str = path.to_string_lossy().to_string();
    db.conn
        .execute("DELETE FROM document WHERE path = ?", (path_str,))
        .await
        .map_err(|source| CommandError::Db {
            op: "delete_document",
            source,
        })?;
    Ok(())
}

struct ListedDocument {
    path: String,
    doc_type: String,
    total_size_bytes: i64,
    total_size_pages: Option<i64>,
    created_at: String,
}

pub async fn list_text(
    db: &Db,
    snapshot: &StatusSnapshot,
    raw_tags: &[String],
    match_all: bool,
) -> Result<String, CommandError> {
    let docs = gather_listed_documents(db, raw_tags, match_all).await?;
    Ok(format_list(&docs, snapshot))
}

pub async fn list_json(
    db: &Db,
    snapshot: &StatusSnapshot,
    raw_tags: &[String],
    match_all: bool,
) -> Result<String, CommandError> {
    let docs = gather_listed_documents(db, raw_tags, match_all).await?;
    let documents: Vec<serde_json::Value> = docs
        .iter()
        .map(|d| {
            serde_json::json!({
                "path": d.path,
                "type": d.doc_type,
                "total_size_bytes": d.total_size_bytes,
                "total_size_pages": d.total_size_pages,
                "created_at": d.created_at,
            })
        })
        .collect();
    let env = serde_json::json!({
        "ok": true,
        "documents": documents,
        "current": snapshot.current.as_ref().map(|c| serde_json::json!({
            "label": c.label,
            "running_secs": c.running_secs,
        })),
        "queued": snapshot.queued,
    });
    Ok(env.to_string())
}

/// Build the JSON payload for the `status` request. Wraps the current
/// `StatusSnapshot` in the standard `{"ok": true, ...}` envelope.
pub fn status_json(snapshot: &StatusSnapshot) -> String {
    let env = serde_json::json!({
        "ok": true,
        "uptime_secs": snapshot.uptime_secs,
        "current": snapshot.current.as_ref().map(|c| serde_json::json!({
            "label": c.label,
            "running_secs": c.running_secs,
        })),
        "queued": snapshot.queued,
    });
    env.to_string()
}

/// Wrap a raw extracted text response in the JSON envelope used by the
/// `text` subcommand. The `range` field mirrors the request shape so callers
/// can correlate results with what they asked for.
pub fn text_to_json(path: &Path, range: &TextRangeReq, raw: &str) -> String {
    let range_json = match range {
        TextRangeReq::Bytes { start, end } => {
            serde_json::json!({"type": "bytes", "start": start, "end": end})
        }
        TextRangeReq::Chars { start, end } => {
            serde_json::json!({"type": "chars", "start": start, "end": end})
        }
        TextRangeReq::Pages { first, last } => {
            serde_json::json!({"type": "pages", "first": first, "last": last})
        }
    };
    serde_json::json!({
        "ok": true,
        "path": path.display().to_string(),
        "range": range_json,
        "text": raw,
    })
    .to_string()
}

async fn gather_listed_documents(
    db: &Db,
    raw_tags: &[String],
    match_all: bool,
) -> Result<Vec<ListedDocument>, CommandError> {
    // Use a fresh connection so this SELECT does not interleave with whatever
    // transaction is open on the worker's `db.conn`.
    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;

    let tags = normalize_tags(raw_tags)?;

    let mut rows = if tags.is_empty() {
        conn.query(
            "SELECT path, doc_type, total_size_bytes, total_size_pages, created_at \
             FROM document ORDER BY created_at",
            (),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "list_text.query",
            source,
        })?
    } else {
        let placeholders = std::iter::repeat("?")
            .take(tags.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = if match_all {
            format!(
                "SELECT d.path, d.doc_type, d.total_size_bytes, d.total_size_pages, d.created_at \
                 FROM document d JOIN document_tag t ON t.document_id = d.id \
                 WHERE t.tag IN ({placeholders}) \
                 GROUP BY d.id \
                 HAVING COUNT(DISTINCT t.tag) = ? \
                 ORDER BY d.created_at"
            )
        } else {
            format!(
                "SELECT d.path, d.doc_type, d.total_size_bytes, d.total_size_pages, d.created_at \
                 FROM document d \
                 WHERE EXISTS (SELECT 1 FROM document_tag t \
                                WHERE t.document_id = d.id AND t.tag IN ({placeholders})) \
                 ORDER BY d.created_at"
            )
        };
        let mut params: Vec<Value> = tags.iter().map(|t| Value::Text(t.clone())).collect();
        if match_all {
            params.push(Value::Integer(tags.len() as i64));
        }
        conn.query(&sql, Params::Positional(params))
            .await
            .map_err(|source| CommandError::Db {
                op: "list_text.query",
                source,
            })?
    };

    let mut docs: Vec<ListedDocument> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "list_text.next",
        source,
    })? {
        let path: String = row.get(0).map_err(|source| CommandError::Db {
            op: "list_text.get.path",
            source,
        })?;
        let doc_type: String = row.get(1).map_err(|source| CommandError::Db {
            op: "list_text.get.doc_type",
            source,
        })?;
        let total_size_bytes: i64 = row.get(2).map_err(|source| CommandError::Db {
            op: "list_text.get.bytes",
            source,
        })?;
        let total_size_pages: Option<i64> = match row.get_value(3) {
            Ok(Value::Null) => None,
            Ok(Value::Integer(n)) => Some(n),
            Ok(other) => panic!("unexpected total_size_pages value {other:?}"),
            Err(source) => {
                return Err(CommandError::Db {
                    op: "list_text.get.pages",
                    source,
                });
            }
        };
        let created_at: String = row.get(4).map_err(|source| CommandError::Db {
            op: "list_text.get.created_at",
            source,
        })?;
        docs.push(ListedDocument {
            path,
            doc_type,
            total_size_bytes,
            total_size_pages,
            created_at,
        });
    }

    Ok(docs)
}

fn format_list(docs: &[ListedDocument], snapshot: &StatusSnapshot) -> String {
    let mut out = String::new();

    let path_w = docs.iter().map(|d| d.path.len()).max().unwrap_or(0);
    let type_w = docs.iter().map(|d| d.doc_type.len()).max().unwrap_or(0).max(3);
    let bytes_w = docs
        .iter()
        .map(|d| d.total_size_bytes.to_string().len())
        .max()
        .unwrap_or(0);
    let pages_w = docs
        .iter()
        .filter_map(|d| d.total_size_pages.map(|p| p.to_string().len()))
        .max()
        .unwrap_or(0);

    let _ = writeln!(out, "ingested ({}):", docs.len());
    for d in docs {
        let pages_cell = match d.total_size_pages {
            Some(p) => format!("{p:>w$} pages", w = pages_w),
            None => " ".repeat(pages_w + " pages".len()),
        };
        let _ = writeln!(
            out,
            "  {path:<path_w$}  {ty:<type_w$}  {bytes:>bytes_w$} bytes  {pages_cell}  {created}",
            path = d.path,
            ty = d.doc_type,
            bytes = d.total_size_bytes,
            created = d.created_at,
        );
    }

    if let Some(c) = &snapshot.current {
        let _ = writeln!(out);
        let _ = writeln!(out, "in progress:");
        let _ = writeln!(out, "  {} (running {}s)", c.label, c.running_secs);
    }

    if !snapshot.queued.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "queued ({}):", snapshot.queued.len());
        for (i, q) in snapshot.queued.iter().enumerate() {
            let _ = writeln!(out, "  {}. {q}", i + 1);
        }
    }

    out
}

pub async fn text_bytes(
    db: &Db,
    raw_path: &Path,
    start: u64,
    end: u64,
) -> Result<String, CommandError> {
    if start > end {
        return Err(CommandError::InvalidRange { start, end });
    }
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let total = row.total_size_bytes as u64;
    if end >= total {
        return Err(CommandError::ByteRangeOutOfBounds { start, end, total });
    }
    let end_exclusive = end + 1;
    stitch_bytes(db, row.id, start, end_exclusive).await
}

pub async fn text_chars(
    db: &Db,
    raw_path: &Path,
    start: u64,
    end: u64,
) -> Result<String, CommandError> {
    if start > end {
        return Err(CommandError::InvalidRange { start, end });
    }
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let total = row.total_size_chars as u64;
    if end >= total {
        return Err(CommandError::CharRangeOutOfBounds { start, end, total });
    }
    let end_exclusive = end + 1;
    stitch_chars(db, row.id, start, end_exclusive).await
}

pub async fn text_pages(
    db: &Db,
    raw_path: &Path,
    first: u32,
    last: u32,
) -> Result<String, CommandError> {
    if first > last {
        return Err(CommandError::InvalidPageRange { first, last });
    }
    if first == 0 {
        return Err(CommandError::PageRangeOutOfBounds {
            first,
            last,
            total: 0,
        });
    }
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;
    let total_pages = row.total_size_pages.ok_or(CommandError::NotPdf {
        path: path.clone(),
    })? as u32;
    if last > total_pages {
        return Err(CommandError::PageRangeOutOfBounds {
            first,
            last,
            total: total_pages,
        });
    }

    let (byte_start, byte_end_exclusive) = page_byte_range(db, row.id, first, last).await?;
    stitch_bytes(db, row.id, byte_start, byte_end_exclusive).await
}

fn canonicalize(p: &Path) -> Result<PathBuf, CommandError> {
    p.canonicalize().map_err(|source| CommandError::Canonicalize {
        path: p.to_path_buf(),
        source,
    })
}

async fn lookup_document(db: &Db, path: &Path) -> Result<DocumentRow, CommandError> {
    let path_str = path.to_string_lossy().to_string();
    let mut rows = db
        .conn
        .query(
            "SELECT id, doc_type, total_size_bytes, total_size_chars, total_size_pages \
             FROM document WHERE path = ?",
            (path_str,),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "lookup_document",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| CommandError::Db {
            op: "lookup_document.next",
            source,
        })?
        .ok_or_else(|| CommandError::DocumentNotFound {
            path: path.to_path_buf(),
        })?;

    let id: i64 = row.get(0).map_err(|source| CommandError::Db {
        op: "lookup_document.get.id",
        source,
    })?;
    let doc_type: String = row.get(1).map_err(|source| CommandError::Db {
        op: "lookup_document.get.doc_type",
        source,
    })?;
    let total_size_bytes: i64 = row.get(2).map_err(|source| CommandError::Db {
        op: "lookup_document.get.bytes",
        source,
    })?;
    let total_size_chars: i64 = row.get(3).map_err(|source| CommandError::Db {
        op: "lookup_document.get.chars",
        source,
    })?;
    let total_size_pages: Option<i64> = match row.get_value(4) {
        Ok(Value::Null) => None,
        Ok(Value::Integer(n)) => Some(n),
        Ok(other) => panic!("unexpected total_size_pages value {other:?}"),
        Err(source) => {
            return Err(CommandError::Db {
                op: "lookup_document.get.pages",
                source,
            });
        }
    };

    Ok(DocumentRow {
        id,
        doc_type,
        total_size_bytes,
        total_size_chars,
        total_size_pages,
    })
}

async fn page_byte_range(
    db: &Db,
    document_id: i64,
    first: u32,
    last: u32,
) -> Result<(u64, u64), CommandError> {
    let mut rows = db
        .conn
        .query(
            "SELECT MIN(byte_start), MAX(byte_end) FROM document_page \
             WHERE document_id = ? AND page_number BETWEEN ? AND ?",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(first as i64),
                Value::Integer(last as i64),
            ]),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "page_byte_range",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| CommandError::Db {
            op: "page_byte_range.next",
            source,
        })?
        .expect("MIN/MAX query always returns one row");
    let bs: i64 = row.get(0).map_err(|source| CommandError::Db {
        op: "page_byte_range.get.bs",
        source,
    })?;
    let be: i64 = row.get(1).map_err(|source| CommandError::Db {
        op: "page_byte_range.get.be",
        source,
    })?;
    Ok((bs as u64, be as u64))
}

struct ChunkRow {
    byte_start: u64,
    byte_end: u64,
    char_start: u64,
    char_end: u64,
    text: String,
}

async fn fetch_chunks_byte(
    db: &Db,
    document_id: i64,
    start: u64,
    end_exclusive: u64,
) -> Result<Vec<ChunkRow>, CommandError> {
    let mut rows = db
        .conn
        .query(
            "SELECT byte_start, byte_end, char_start, char_end, text \
             FROM document_chunk \
             WHERE document_id = ? AND byte_start < ? AND byte_end > ? \
             ORDER BY chunk_index",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(end_exclusive as i64),
                Value::Integer(start as i64),
            ]),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "fetch_chunks_byte",
            source,
        })?;
    collect_chunks(&mut rows).await
}

async fn fetch_chunks_char(
    db: &Db,
    document_id: i64,
    start: u64,
    end_exclusive: u64,
) -> Result<Vec<ChunkRow>, CommandError> {
    let mut rows = db
        .conn
        .query(
            "SELECT byte_start, byte_end, char_start, char_end, text \
             FROM document_chunk \
             WHERE document_id = ? AND char_start < ? AND char_end > ? \
             ORDER BY chunk_index",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(end_exclusive as i64),
                Value::Integer(start as i64),
            ]),
        )
        .await
        .map_err(|source| CommandError::Db {
            op: "fetch_chunks_char",
            source,
        })?;
    collect_chunks(&mut rows).await
}

async fn collect_chunks(rows: &mut turso::Rows) -> Result<Vec<ChunkRow>, CommandError> {
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "collect_chunks.next",
        source,
    })? {
        let byte_start: i64 = row.get(0).map_err(|source| CommandError::Db {
            op: "collect_chunks.get.byte_start",
            source,
        })?;
        let byte_end: i64 = row.get(1).map_err(|source| CommandError::Db {
            op: "collect_chunks.get.byte_end",
            source,
        })?;
        let char_start: i64 = row.get(2).map_err(|source| CommandError::Db {
            op: "collect_chunks.get.char_start",
            source,
        })?;
        let char_end: i64 = row.get(3).map_err(|source| CommandError::Db {
            op: "collect_chunks.get.char_end",
            source,
        })?;
        let text: String = row.get(4).map_err(|source| CommandError::Db {
            op: "collect_chunks.get.text",
            source,
        })?;
        out.push(ChunkRow {
            byte_start: byte_start as u64,
            byte_end: byte_end as u64,
            char_start: char_start as u64,
            char_end: char_end as u64,
            text,
        });
    }
    Ok(out)
}

async fn stitch_bytes(
    db: &Db,
    document_id: i64,
    start: u64,
    end_exclusive: u64,
) -> Result<String, CommandError> {
    let chunks = fetch_chunks_byte(db, document_id, start, end_exclusive).await?;
    let mut out = String::new();
    let mut cursor = start;
    for c in chunks {
        if c.byte_end <= cursor {
            continue;
        }
        if c.byte_start > cursor {
            return Err(CommandError::ChunkCoverageGap { missing: cursor });
        }
        let take_from = (cursor - c.byte_start) as usize;
        let take_to = (end_exclusive.min(c.byte_end) - c.byte_start) as usize;
        out.push_str(&c.text[take_from..take_to]);
        cursor = c.byte_end.min(end_exclusive);
        if cursor >= end_exclusive {
            break;
        }
    }
    if cursor < end_exclusive {
        return Err(CommandError::ChunkCoverageGap { missing: cursor });
    }
    Ok(out)
}

async fn stitch_chars(
    db: &Db,
    document_id: i64,
    start: u64,
    end_exclusive: u64,
) -> Result<String, CommandError> {
    let chunks = fetch_chunks_char(db, document_id, start, end_exclusive).await?;
    let mut out = String::new();
    let mut cursor = start;
    for c in chunks {
        if c.char_end <= cursor {
            continue;
        }
        if c.char_start > cursor {
            return Err(CommandError::ChunkCoverageGap { missing: cursor });
        }
        let take_from = (cursor - c.char_start) as usize;
        let take_to = (end_exclusive.min(c.char_end) - c.char_start) as usize;
        let slice: String = c.text.chars().skip(take_from).take(take_to - take_from).collect();
        out.push_str(&slice);
        cursor = c.char_end.min(end_exclusive);
        if cursor >= end_exclusive {
            break;
        }
    }
    if cursor < end_exclusive {
        return Err(CommandError::ChunkCoverageGap { missing: cursor });
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitSource {
    Chunk,
    Summary,
    Both,
}

impl HitSource {
    fn as_str(&self) -> &'static str {
        match self {
            HitSource::Chunk => "chunk",
            HitSource::Summary => "summary",
            HitSource::Both => "both",
        }
    }

    fn merged_with(self, other: HitSource) -> HitSource {
        if self == other {
            self
        } else {
            HitSource::Both
        }
    }
}

struct SearchHit {
    document_id: i64,
    chunk_id: i64,
    path: String,
    byte_start: i64,
    byte_end: i64,
    page_first: Option<i64>,
    page_last: Option<i64>,
    text: String,
    similarity: f32,
    source: HitSource,
}

/// Per-document group used by the `--include-summaries` rendering path.
struct DocGroup {
    path: String,
    region_summary: Option<String>,
    region_summary_level: Option<i64>,
    hits: Vec<SearchHit>,
}

#[allow(clippy::too_many_arguments)]
pub async fn search_text(
    db: &Db,
    http: &reqwest::Client,
    cfg: &Config,
    term: &str,
    path: Option<&Path>,
    raw_tags: &[String],
    match_all: bool,
    limit_override: Option<usize>,
    cutoff_override: Option<f32>,
    no_truncate: bool,
    include_summaries: bool,
) -> Result<String, CommandError> {
    let (hits, limit, cutoff) = gather_search_hits(
        db,
        http,
        cfg,
        term,
        path,
        raw_tags,
        match_all,
        limit_override,
        cutoff_override,
        include_summaries,
    )
    .await?;
    if include_summaries {
        let groups = group_hits_by_document(db, &hits, &cutoff).await?;
        Ok(render_search_grouped(term, limit, cutoff, &groups, no_truncate))
    } else {
        Ok(render_search(term, limit, cutoff, &hits, no_truncate))
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn search_json(
    db: &Db,
    http: &reqwest::Client,
    cfg: &Config,
    term: &str,
    path: Option<&Path>,
    raw_tags: &[String],
    match_all: bool,
    limit_override: Option<usize>,
    cutoff_override: Option<f32>,
    no_truncate: bool,
    include_summaries: bool,
) -> Result<String, CommandError> {
    let (hits, limit, cutoff) = gather_search_hits(
        db,
        http,
        cfg,
        term,
        path,
        raw_tags,
        match_all,
        limit_override,
        cutoff_override,
        include_summaries,
    )
    .await?;
    let max_chars = if no_truncate { None } else { Some(SNIPPET_MAX_CHARS) };
    if include_summaries {
        let groups = group_hits_by_document(db, &hits, &cutoff).await?;
        let documents: Vec<serde_json::Value> = groups
            .iter()
            .map(|g| {
                let hits_json: Vec<serde_json::Value> = g
                    .hits
                    .iter()
                    .map(|h| {
                        let (snippet_text, truncated) = snippet_with_flag(&h.text, max_chars);
                        serde_json::json!({
                            "byte_start": h.byte_start,
                            "byte_end": h.byte_end,
                            "page_first": h.page_first,
                            "page_last": h.page_last,
                            "similarity": h.similarity,
                            "snippet": snippet_text,
                            "truncated": truncated,
                            "source": h.source.as_str(),
                        })
                    })
                    .collect();
                serde_json::json!({
                    "path": g.path,
                    "region_summary": g.region_summary,
                    "region_summary_level": g.region_summary_level,
                    "hits": hits_json,
                })
            })
            .collect();
        let env = serde_json::json!({
            "ok": true,
            "term": term,
            "limit": limit,
            "cutoff": cutoff,
            "include_summaries": true,
            "documents": documents,
        });
        Ok(env.to_string())
    } else {
        let results: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                let (snippet_text, truncated) = snippet_with_flag(&h.text, max_chars);
                serde_json::json!({
                    "path": h.path,
                    "byte_start": h.byte_start,
                    "byte_end": h.byte_end,
                    "page_first": h.page_first,
                    "page_last": h.page_last,
                    "similarity": h.similarity,
                    "snippet": snippet_text,
                    "truncated": truncated,
                })
            })
            .collect();
        let env = serde_json::json!({
            "ok": true,
            "term": term,
            "limit": limit,
            "cutoff": cutoff,
            "results": results,
        });
        Ok(env.to_string())
    }
}

const SNIPPET_MAX_CHARS: usize = 240;

#[allow(clippy::too_many_arguments)]
async fn gather_search_hits(
    db: &Db,
    http: &reqwest::Client,
    cfg: &Config,
    term: &str,
    path: Option<&Path>,
    raw_tags: &[String],
    match_all: bool,
    limit_override: Option<usize>,
    cutoff_override: Option<f32>,
    include_summaries: bool,
) -> Result<(Vec<SearchHit>, usize, f32), CommandError> {
    if path.is_none() && raw_tags.is_empty() {
        return Err(CommandError::SearchScopeRequired);
    }

    let limit = limit_override.unwrap_or(cfg.search.default_results_per_document);
    if limit == 0 {
        return Err(CommandError::InvalidLimit);
    }
    let cutoff = cutoff_override.unwrap_or(cfg.search.relevancy_cutoff);
    if !(0.0..=1.0).contains(&cutoff) {
        return Err(CommandError::InvalidCutoff(cutoff));
    }
    let max_distance = (1.0 - cutoff) as f64;

    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;

    let docs = if let Some(p) = path {
        let canon = canonicalize(p)?;
        let path_str = canon.to_string_lossy().to_string();
        let mut rows = conn
            .query("SELECT id, path FROM document WHERE path = ?", (path_str,))
            .await
            .map_err(|source| CommandError::Db {
                op: "search.lookup_path",
                source,
            })?;
        let row = rows
            .next()
            .await
            .map_err(|source| CommandError::Db {
                op: "search.lookup_path.next",
                source,
            })?
            .ok_or_else(|| CommandError::DocumentNotFound {
                path: canon.clone(),
            })?;
        let id: i64 = row.get(0).map_err(|source| CommandError::Db {
            op: "search.lookup_path.get.id",
            source,
        })?;
        let path: String = row.get(1).map_err(|source| CommandError::Db {
            op: "search.lookup_path.get.path",
            source,
        })?;
        vec![(id, path)]
    } else {
        let tags = normalize_tags(raw_tags)?;
        let placeholders = std::iter::repeat("?")
            .take(tags.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = if match_all {
            format!(
                "SELECT d.id, d.path FROM document d \
                 JOIN document_tag t ON t.document_id = d.id \
                 WHERE t.tag IN ({placeholders}) \
                 GROUP BY d.id \
                 HAVING COUNT(DISTINCT t.tag) = ?"
            )
        } else {
            format!(
                "SELECT d.id, d.path FROM document d \
                 WHERE EXISTS (SELECT 1 FROM document_tag t \
                                WHERE t.document_id = d.id AND t.tag IN ({placeholders}))"
            )
        };
        let mut params: Vec<Value> = tags.iter().map(|t| Value::Text(t.clone())).collect();
        if match_all {
            params.push(Value::Integer(tags.len() as i64));
        }
        let mut rows = conn
            .query(&sql, Params::Positional(params))
            .await
            .map_err(|source| CommandError::Db {
                op: "search.lookup_tags",
                source,
            })?;
        let mut docs: Vec<(i64, String)> = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
            op: "search.lookup_tags.next",
            source,
        })? {
            let id: i64 = row.get(0).map_err(|source| CommandError::Db {
                op: "search.lookup_tags.get.id",
                source,
            })?;
            let p: String = row.get(1).map_err(|source| CommandError::Db {
                op: "search.lookup_tags.get.path",
                source,
            })?;
            docs.push((id, p));
        }
        docs
    };

    let embedding = ollama::embed(http, &cfg.ollama.url, &cfg.ollama.embedding_model, term)
        .await
        .map_err(|source| CommandError::EmbedQuery { source })?;
    if embedding.len() != db.embedding_dimensions {
        return Err(CommandError::EmbedDimMismatch {
            got: embedding.len(),
            expected: db.embedding_dimensions,
            model: db.embedding_model.clone(),
        });
    }
    let blob = vector::to_blob(&embedding);

    let per_doc_sql = format!(
        "SELECT \
            c.id, c.byte_start, c.byte_end, c.page_first, c.page_last, c.text, \
            vector_distance_cos(cv.embedding, ?) AS distance \
         FROM document_chunk c \
         JOIN {chunks_table} cv ON cv.chunk_id = c.id \
         WHERE c.document_id = ? \
           AND vector_distance_cos(cv.embedding, ?) <= ? \
         ORDER BY distance ASC \
         LIMIT ?",
        chunks_table = db.chunks_table,
    );

    let mut hits: Vec<SearchHit> = Vec::new();
    for (doc_id, path) in &docs {
        let params = Params::Positional(vec![
            Value::Blob(blob.clone()),
            Value::Integer(*doc_id),
            Value::Blob(blob.clone()),
            Value::Real(max_distance),
            Value::Integer(limit as i64),
        ]);
        let mut rows = conn
            .query(&per_doc_sql, params)
            .await
            .map_err(|source| CommandError::Db {
                op: "search.per_doc",
                source,
            })?;
        while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
            op: "search.per_doc.next",
            source,
        })? {
            let chunk_id: i64 = row.get(0).map_err(|source| CommandError::Db {
                op: "search.per_doc.get.chunk_id",
                source,
            })?;
            let byte_start: i64 = row.get(1).map_err(|source| CommandError::Db {
                op: "search.per_doc.get.byte_start",
                source,
            })?;
            let byte_end: i64 = row.get(2).map_err(|source| CommandError::Db {
                op: "search.per_doc.get.byte_end",
                source,
            })?;
            let page_first: Option<i64> = match row.get_value(3) {
                Ok(Value::Null) => None,
                Ok(Value::Integer(n)) => Some(n),
                Ok(other) => panic!("unexpected page_first value {other:?}"),
                Err(source) => {
                    return Err(CommandError::Db {
                        op: "search.per_doc.get.page_first",
                        source,
                    });
                }
            };
            let page_last: Option<i64> = match row.get_value(4) {
                Ok(Value::Null) => None,
                Ok(Value::Integer(n)) => Some(n),
                Ok(other) => panic!("unexpected page_last value {other:?}"),
                Err(source) => {
                    return Err(CommandError::Db {
                        op: "search.per_doc.get.page_last",
                        source,
                    });
                }
            };
            let text: String = row.get(5).map_err(|source| CommandError::Db {
                op: "search.per_doc.get.text",
                source,
            })?;
            let distance: f64 = row.get(6).map_err(|source| CommandError::Db {
                op: "search.per_doc.get.distance",
                source,
            })?;
            let similarity = (1.0 - distance as f32).clamp(0.0, 1.0);
            hits.push(SearchHit {
                document_id: *doc_id,
                chunk_id,
                path: path.clone(),
                byte_start,
                byte_end,
                page_first,
                page_last,
                text,
                similarity,
                source: HitSource::Chunk,
            });
        }
    }

    if include_summaries {
        let summary_hits =
            gather_summary_hits(db, &conn, &blob, &docs, max_distance, limit).await?;
        merge_summary_hits(&mut hits, summary_hits);
    }

    hits.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(Ordering::Equal)
    });

    Ok((hits, limit, cutoff))
}

/// Top-K vector search against `summary_vectors_<N>` for the in-scope docs,
/// then expand each matching summary to the underlying chunks (one batched
/// query per document). Each emitted hit carries the *summary's* similarity,
/// not the chunk's own — those will get max-merged with chunk-search hits.
async fn gather_summary_hits(
    db: &Db,
    conn: &turso::Connection,
    query_blob: &[u8],
    docs: &[(i64, String)],
    max_distance: f64,
    limit: usize,
) -> Result<Vec<SearchHit>, CommandError> {
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    // Cap the summary candidates per call. Generous so summary-rich docs
    // still surface their best regions; the per-doc chunk-limit kicks in
    // downstream when results are grouped.
    let summary_limit = (limit * docs.len() * 3).max(10) as i64;

    let placeholders = std::iter::repeat("?")
        .take(docs.len())
        .collect::<Vec<_>>()
        .join(", ");
    let summary_sql = format!(
        "SELECT s.id, s.document_id, s.level, s.byte_start, s.byte_end, s.text, \
                vector_distance_cos(sv.embedding, ?) AS distance \
         FROM document_summary s \
         JOIN {summary_vectors} sv ON sv.summary_id = s.id \
         WHERE s.document_id IN ({placeholders}) \
           AND vector_distance_cos(sv.embedding, ?) <= ? \
         ORDER BY distance ASC \
         LIMIT ?",
        summary_vectors = db.summary_vectors_table,
    );

    let mut params: Vec<Value> = Vec::with_capacity(docs.len() + 4);
    params.push(Value::Blob(query_blob.to_vec()));
    for (id, _) in docs {
        params.push(Value::Integer(*id));
    }
    params.push(Value::Blob(query_blob.to_vec()));
    params.push(Value::Real(max_distance));
    params.push(Value::Integer(summary_limit));

    let mut rows = conn
        .query(&summary_sql, Params::Positional(params))
        .await
        .map_err(|source| CommandError::Db {
            op: "search.summary",
            source,
        })?;

    // Collect summaries grouped by document so we can expand each doc's
    // covering byte ranges in one query below.
    struct TopSummary {
        document_id: i64,
        byte_start: i64,
        byte_end: i64,
        similarity: f32,
    }
    let mut summaries: Vec<TopSummary> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
        op: "search.summary.next",
        source,
    })? {
        let _id: i64 = row.get(0).map_err(|source| CommandError::Db {
            op: "search.summary.get.id",
            source,
        })?;
        let document_id: i64 = row.get(1).map_err(|source| CommandError::Db {
            op: "search.summary.get.document_id",
            source,
        })?;
        let _level: i64 = row.get(2).map_err(|source| CommandError::Db {
            op: "search.summary.get.level",
            source,
        })?;
        let byte_start: i64 = row.get(3).map_err(|source| CommandError::Db {
            op: "search.summary.get.byte_start",
            source,
        })?;
        let byte_end: i64 = row.get(4).map_err(|source| CommandError::Db {
            op: "search.summary.get.byte_end",
            source,
        })?;
        let _text: String = row.get(5).map_err(|source| CommandError::Db {
            op: "search.summary.get.text",
            source,
        })?;
        let distance: f64 = row.get(6).map_err(|source| CommandError::Db {
            op: "search.summary.get.distance",
            source,
        })?;
        let similarity = (1.0 - distance as f32).clamp(0.0, 1.0);
        summaries.push(TopSummary {
            document_id,
            byte_start,
            byte_end,
            similarity,
        });
    }

    // Group summaries by doc and pick the best (max) similarity per byte
    // range. Then for each doc, expand to chunks within those ranges.
    let mut by_doc: std::collections::HashMap<i64, Vec<TopSummary>> =
        std::collections::HashMap::new();
    for s in summaries {
        by_doc.entry(s.document_id).or_default().push(s);
    }

    let mut out: Vec<SearchHit> = Vec::new();
    for (doc_id, doc_summaries) in by_doc {
        let path = docs
            .iter()
            .find(|(id, _)| *id == doc_id)
            .map(|(_, p)| p.clone())
            .unwrap_or_default();

        // Pull all chunks within any covered range in one query. We union the
        // ranges via OR in SQL — small N (summary_limit), so OK.
        let mut where_clauses = Vec::with_capacity(doc_summaries.len());
        let mut chunk_params: Vec<Value> = Vec::new();
        chunk_params.push(Value::Integer(doc_id));
        for s in &doc_summaries {
            where_clauses.push("(byte_start >= ? AND byte_end <= ?)".to_string());
            chunk_params.push(Value::Integer(s.byte_start));
            chunk_params.push(Value::Integer(s.byte_end));
        }
        let chunks_sql = format!(
            "SELECT id, byte_start, byte_end, page_first, page_last, text \
             FROM document_chunk \
             WHERE document_id = ? AND ({})",
            where_clauses.join(" OR ")
        );

        let mut chunk_rows = conn
            .query(&chunks_sql, Params::Positional(chunk_params))
            .await
            .map_err(|source| CommandError::Db {
                op: "search.summary.expand",
                source,
            })?;
        while let Some(row) = chunk_rows.next().await.map_err(|source| CommandError::Db {
            op: "search.summary.expand.next",
            source,
        })? {
            let chunk_id: i64 = row.get(0).map_err(|source| CommandError::Db {
                op: "search.summary.expand.get.chunk_id",
                source,
            })?;
            let byte_start: i64 = row.get(1).map_err(|source| CommandError::Db {
                op: "search.summary.expand.get.byte_start",
                source,
            })?;
            let byte_end: i64 = row.get(2).map_err(|source| CommandError::Db {
                op: "search.summary.expand.get.byte_end",
                source,
            })?;
            let page_first: Option<i64> = match row.get_value(3) {
                Ok(Value::Null) => None,
                Ok(Value::Integer(n)) => Some(n),
                Ok(other) => panic!("unexpected page_first value {other:?}"),
                Err(source) => {
                    return Err(CommandError::Db {
                        op: "search.summary.expand.get.page_first",
                        source,
                    });
                }
            };
            let page_last: Option<i64> = match row.get_value(4) {
                Ok(Value::Null) => None,
                Ok(Value::Integer(n)) => Some(n),
                Ok(other) => panic!("unexpected page_last value {other:?}"),
                Err(source) => {
                    return Err(CommandError::Db {
                        op: "search.summary.expand.get.page_last",
                        source,
                    });
                }
            };
            let text: String = row.get(5).map_err(|source| CommandError::Db {
                op: "search.summary.expand.get.text",
                source,
            })?;
            // Best summary similarity that covers this chunk. With small N
            // this scan is cheap.
            let similarity = doc_summaries
                .iter()
                .filter(|s| s.byte_start <= byte_start && s.byte_end >= byte_end)
                .map(|s| s.similarity)
                .fold(0.0f32, f32::max);
            out.push(SearchHit {
                document_id: doc_id,
                chunk_id,
                path: path.clone(),
                byte_start,
                byte_end,
                page_first,
                page_last,
                text,
                similarity,
                source: HitSource::Summary,
            });
        }
    }
    Ok(out)
}

/// Merge summary-expanded hits into the chunk-search hits, deduping by
/// `(document_id, chunk_id)`. When the same chunk appears in both sets we
/// keep the higher similarity and flag the source as `Both`.
fn merge_summary_hits(hits: &mut Vec<SearchHit>, summary_hits: Vec<SearchHit>) {
    use std::collections::HashMap;
    let mut index: HashMap<(i64, i64), usize> = HashMap::with_capacity(hits.len());
    for (i, h) in hits.iter().enumerate() {
        index.insert((h.document_id, h.chunk_id), i);
    }
    for s in summary_hits {
        let key = (s.document_id, s.chunk_id);
        match index.get(&key) {
            Some(&i) => {
                let h = &mut hits[i];
                if s.similarity > h.similarity {
                    h.similarity = s.similarity;
                }
                h.source = h.source.merged_with(s.source);
            }
            None => {
                index.insert(key, hits.len());
                hits.push(s);
            }
        }
    }
}

/// Group merged hits by document and pull each document's smallest covering
/// level≥1 summary in one query per doc. The region summary is the smallest
/// (innermost) covering one — narrower context is more relevant.
async fn group_hits_by_document(
    db: &Db,
    hits: &[SearchHit],
    _cutoff: &f32,
) -> Result<Vec<DocGroup>, CommandError> {
    // Bucket hits by document_id while preserving relative order (already
    // similarity-desc from the caller). The first bucket key seen wins doc
    // ordering, which makes the top-hit doc come first.
    use std::collections::HashMap;
    let mut doc_order: Vec<i64> = Vec::new();
    let mut buckets: HashMap<i64, (String, Vec<usize>)> = HashMap::new();
    for (i, h) in hits.iter().enumerate() {
        buckets
            .entry(h.document_id)
            .or_insert_with(|| {
                doc_order.push(h.document_id);
                (h.path.clone(), Vec::new())
            })
            .1
            .push(i);
    }

    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;

    let mut groups: Vec<DocGroup> = Vec::with_capacity(doc_order.len());
    for doc_id in doc_order {
        let (path, idxs) = buckets.remove(&doc_id).expect("bucket exists");
        // Pull all level>=1 summaries for this doc once; pick covering ones
        // per hit in memory below.
        let mut rows = conn
            .query(
                "SELECT level, byte_start, byte_end, text \
                 FROM document_summary \
                 WHERE document_id = ? AND level >= 1 \
                 ORDER BY level ASC, byte_start ASC",
                (doc_id,),
            )
            .await
            .map_err(|source| CommandError::Db {
                op: "search.region_summary",
                source,
            })?;
        struct LevelSummary {
            level: i64,
            byte_start: i64,
            byte_end: i64,
            text: String,
        }
        let mut summaries: Vec<LevelSummary> = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| CommandError::Db {
            op: "search.region_summary.next",
            source,
        })? {
            let level: i64 = row.get(0).map_err(|source| CommandError::Db {
                op: "search.region_summary.get.level",
                source,
            })?;
            let byte_start: i64 = row.get(1).map_err(|source| CommandError::Db {
                op: "search.region_summary.get.byte_start",
                source,
            })?;
            let byte_end: i64 = row.get(2).map_err(|source| CommandError::Db {
                op: "search.region_summary.get.byte_end",
                source,
            })?;
            let text: String = row.get(3).map_err(|source| CommandError::Db {
                op: "search.region_summary.get.text",
                source,
            })?;
            summaries.push(LevelSummary {
                level,
                byte_start,
                byte_end,
                text,
            });
        }

        // Pick the region summary for this doc: smallest-level covering
        // summary for the doc's top-scoring hit. Falls back to None if the
        // doc has no summaries built yet.
        let region: Option<&LevelSummary> = idxs
            .first()
            .and_then(|&i| {
                let hit = &hits[i];
                summaries
                    .iter()
                    .filter(|s| s.byte_start <= hit.byte_start && s.byte_end >= hit.byte_end)
                    // Already sorted by level ASC, so first match is the innermost.
                    .next()
            });

        let region_summary = region.map(|s| s.text.clone());
        let region_summary_level = region.map(|s| s.level);

        // Materialize hits, preserving the similarity-desc order from the
        // caller. We move them out by index; the caller is fine with that
        // because hits is borrowed read-only and we clone.
        let hits_owned: Vec<SearchHit> = idxs
            .into_iter()
            .map(|i| {
                let h = &hits[i];
                SearchHit {
                    document_id: h.document_id,
                    chunk_id: h.chunk_id,
                    path: h.path.clone(),
                    byte_start: h.byte_start,
                    byte_end: h.byte_end,
                    page_first: h.page_first,
                    page_last: h.page_last,
                    text: h.text.clone(),
                    similarity: h.similarity,
                    source: h.source,
                }
            })
            .collect();

        groups.push(DocGroup {
            path,
            region_summary,
            region_summary_level,
            hits: hits_owned,
        });
    }

    Ok(groups)
}

fn render_search(
    term: &str,
    limit: usize,
    cutoff: f32,
    hits: &[SearchHit],
    no_truncate: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "search {:?} \u{2014} {} result(s) (cutoff {:.3}, limit {}/doc)",
        term,
        hits.len(),
        cutoff,
        limit,
    );
    if hits.is_empty() {
        return out;
    }
    let max_chars = if no_truncate { None } else { Some(SNIPPET_MAX_CHARS) };
    for h in hits {
        let _ = writeln!(out);
        let location = format_location(h.page_first, h.page_last, h.byte_start, h.byte_end);
        let _ = writeln!(
            out,
            "{}  {}  similarity {:.4}",
            h.path, location, h.similarity,
        );
        let (snip, _) = snippet_with_flag(&h.text, max_chars);
        let _ = writeln!(out, "  {snip}");
    }
    out
}

fn format_location(
    page_first: Option<i64>,
    page_last: Option<i64>,
    byte_start: i64,
    byte_end: i64,
) -> String {
    match page_first {
        Some(first) => match page_last {
            Some(last) if last != first => format!("page {first}-{last}"),
            _ => format!("page {first}"),
        },
        None => format!("bytes {byte_start}-{byte_end}"),
    }
}

fn render_search_grouped(
    term: &str,
    limit: usize,
    cutoff: f32,
    groups: &[DocGroup],
    no_truncate: bool,
) -> String {
    let total_hits: usize = groups.iter().map(|g| g.hits.len()).sum();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "search {:?} \u{2014} {} result(s) across {} doc(s) (cutoff {:.3}, limit {}/doc, +summaries)",
        term,
        total_hits,
        groups.len(),
        cutoff,
        limit,
    );
    if groups.is_empty() {
        return out;
    }
    let max_chars = if no_truncate { None } else { Some(SNIPPET_MAX_CHARS) };
    let summary_max_chars = if no_truncate { None } else { Some(800) };

    for g in groups {
        let _ = writeln!(out);
        let _ = writeln!(out, "=== {} ===", g.path);
        match (&g.region_summary, g.region_summary_level) {
            (Some(text), Some(level)) => {
                let (snip, _) = snippet_with_flag(text, summary_max_chars);
                let _ = writeln!(out, "  region summary (level {level}): {snip}");
            }
            _ => {
                let _ = writeln!(out, "  region summary: (none built — run `summarize`)");
            }
        }
        for h in &g.hits {
            let location = format_location(h.page_first, h.page_last, h.byte_start, h.byte_end);
            let _ = writeln!(
                out,
                "  - {}  similarity {:.4}  [{}]",
                location,
                h.similarity,
                h.source.as_str(),
            );
            let (snip, _) = snippet_with_flag(&h.text, max_chars);
            let _ = writeln!(out, "      {snip}");
        }
    }
    out
}

/// Whitespace-collapse a chunk body. When `max_chars` is `Some(n)`, also
/// truncate to `n` chars and append U+2026; the second tuple field reports
/// whether truncation happened. With `None`, only collapse and return.
fn snippet_with_flag(text: &str, max_chars: Option<usize>) -> (String, bool) {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match max_chars {
        None => (collapsed, false),
        Some(n) => {
            let total = collapsed.chars().count();
            if total <= n {
                (collapsed, false)
            } else {
                let truncated: String = collapsed.chars().take(n).collect();
                (format!("{truncated}\u{2026}"), true)
            }
        }
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    fn hit(path: &str, sim: f32, page: Option<i64>, text: &str) -> SearchHit {
        SearchHit {
            document_id: 1,
            chunk_id: 1,
            path: path.to_string(),
            byte_start: 0,
            byte_end: 100,
            page_first: page,
            page_last: page,
            text: text.to_string(),
            similarity: sim,
            source: HitSource::Chunk,
        }
    }

    #[test]
    fn render_empty_results() {
        let out = render_search("foo", 5, 0.3, &[], false);
        assert!(out.starts_with("search \"foo\""));
        assert!(out.contains("0 result(s)"));
        assert!(!out.contains("similarity"));
    }

    #[test]
    fn render_mixed_pdf_and_non_pdf() {
        let hits = vec![
            hit("a.pdf", 0.91, Some(3), "from a pdf"),
            hit("b.txt", 0.42, None, "from a text file"),
        ];
        let out = render_search("q", 5, 0.3, &hits, false);
        assert!(out.contains("a.pdf  page 3  similarity 0.9100"));
        assert!(out.contains("b.txt  bytes 0-100  similarity 0.4200"));
    }

    #[test]
    fn snippet_collapses_whitespace_and_truncates() {
        let (s, truncated) = snippet_with_flag("a   b\nc\t d", Some(100));
        assert_eq!(s, "a b c d");
        assert!(!truncated);

        let long: String = "x".repeat(300);
        let (trunc, truncated) = snippet_with_flag(&long, Some(240));
        assert_eq!(trunc.chars().count(), 241); // 240 + ellipsis
        assert!(trunc.ends_with('\u{2026}'));
        assert!(truncated);
    }

    #[test]
    fn snippet_handles_multibyte_boundary() {
        // 4-byte chars: ensure we slice on a char boundary, not bytes.
        let s: String = "\u{1F600}".repeat(300);
        let (trunc, truncated) = snippet_with_flag(&s, Some(240));
        assert_eq!(trunc.chars().count(), 241);
        assert!(truncated);
    }

    #[test]
    fn snippet_no_truncate_returns_full_collapsed_text() {
        let long: String = "x".repeat(300);
        let (full, truncated) = snippet_with_flag(&long, None);
        assert_eq!(full.chars().count(), 300);
        assert!(!full.ends_with('\u{2026}'));
        assert!(!truncated);

        // whitespace still collapsed in no-truncate mode
        let (collapsed, truncated) = snippet_with_flag("a   b\nc\t d", None);
        assert_eq!(collapsed, "a b c d");
        assert!(!truncated);
    }

    #[test]
    fn render_search_no_truncate_keeps_full_snippet() {
        let long_text: String = "y".repeat(300);
        let hits = vec![hit("a.pdf", 0.5, Some(1), &long_text)];
        let out_truncated = render_search("q", 5, 0.3, &hits, false);
        assert!(out_truncated.contains('\u{2026}'));
        let out_full = render_search("q", 5, 0.3, &hits, true);
        assert!(!out_full.contains('\u{2026}'));
    }
}
