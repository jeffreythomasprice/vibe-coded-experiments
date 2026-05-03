//! Read-side CLI command handlers: `info` and the three flavors of `text`.

use std::path::{Path, PathBuf};

use turso::{Value, params::Params};

use crate::db::Db;

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
}

struct DocumentRow {
    id: i64,
    doc_type: String,
    total_size_bytes: i64,
    total_size_chars: i64,
    total_size_pages: Option<i64>,
}

pub async fn info(db: &Db, raw_path: &Path) -> Result<(), CommandError> {
    let path = canonicalize(raw_path)?;
    let row = lookup_document(db, &path).await?;

    println!("path:             {}", path.display());
    println!("type:             {}", row.doc_type);
    println!("total_size_bytes: {}", row.total_size_bytes);
    println!("total_size_chars: {}", row.total_size_chars);
    match row.total_size_pages {
        Some(p) => println!("total_size_pages: {p}"),
        None => println!("total_size_pages: null"),
    }
    Ok(())
}

pub async fn text_bytes(
    db: &Db,
    raw_path: &Path,
    start: u64,
    end: u64,
) -> Result<(), CommandError> {
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
    let out = stitch_bytes(db, row.id, start, end_exclusive).await?;
    print!("{out}");
    Ok(())
}

pub async fn text_chars(
    db: &Db,
    raw_path: &Path,
    start: u64,
    end: u64,
) -> Result<(), CommandError> {
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
    let out = stitch_chars(db, row.id, start, end_exclusive).await?;
    print!("{out}");
    Ok(())
}

pub async fn text_pages(
    db: &Db,
    raw_path: &Path,
    first: u32,
    last: u32,
) -> Result<(), CommandError> {
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
    let out = stitch_bytes(db, row.id, byte_start, byte_end_exclusive).await?;
    print!("{out}");
    Ok(())
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
