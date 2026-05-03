//! Read-side CLI command handlers: `info` and the three flavors of `text`.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use turso::{Value, params::Params};

use crate::db::Db;
use crate::protocol::StatusSnapshot;

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

    Ok(format_list(&docs, snapshot))
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
