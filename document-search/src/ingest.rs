//! End-to-end ingest: read a `.txt` or `.pdf` file, extract canonical text,
//! split into overlapping chunks, embed each chunk, and write everything to
//! the DB inside a single transaction.

use std::path::{Path, PathBuf};

use turso::{Value, params::Params};

use crate::chunking::{self, ChunkSpan};
use crate::config::Config;
use crate::db::{Db, vector};
use crate::ollama;
use crate::pdf::{self, PageOffset, PdfText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocType {
    Txt,
    Pdf,
}

impl DocType {
    fn as_str(self) -> &'static str {
        match self {
            DocType::Txt => "txt",
            DocType::Pdf => "pdf",
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IngestError {
    #[error("canonicalizing path {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("path {path} has no recognized extension; expected .txt or .pdf")]
    UnknownExtension { path: PathBuf },

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("file {path} is not valid UTF-8: {source}")]
    NotUtf8 {
        path: PathBuf,
        #[source]
        source: std::string::FromUtf8Error,
    },

    #[error("extracting text from PDF: {source}")]
    PdfExtract {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("document {path} is already ingested")]
    AlreadyIngested { path: PathBuf },

    #[error("embedding chunk {chunk_index} of {path}: {source}")]
    Embed {
        path: PathBuf,
        chunk_index: usize,
        #[source]
        source: crate::ollama::OllamaError,
    },

    #[error("embedding for chunk {chunk_index} has {got} dims, expected {expected}")]
    EmbedDimMismatch {
        chunk_index: usize,
        got: usize,
        expected: usize,
    },

    #[error("db error during ingest ({op}): {source}")]
    Db {
        op: &'static str,
        #[source]
        source: turso::Error,
    },
}

pub async fn ingest(
    db: &Db,
    cfg: &Config,
    http: &reqwest::Client,
    raw_path: &Path,
) -> Result<i64, IngestError> {
    let path = raw_path
        .canonicalize()
        .map_err(|source| IngestError::Canonicalize {
            path: raw_path.to_path_buf(),
            source,
        })?;

    let doc_type = detect_type(&path)?;
    let bytes = std::fs::read(&path).map_err(|source| IngestError::Read {
        path: path.clone(),
        source,
    })?;

    let (text, page_offsets) = match doc_type {
        DocType::Txt => {
            let s = String::from_utf8(bytes).map_err(|source| IngestError::NotUtf8 {
                path: path.clone(),
                source,
            })?;
            (s, Vec::new())
        }
        DocType::Pdf => {
            let PdfText {
                full_text,
                page_offsets,
            } = pdf::extract(&bytes)?;
            (full_text, page_offsets)
        }
    };

    let total_size_bytes = text.len();
    let total_size_chars = text.chars().count();
    let total_size_pages = if doc_type == DocType::Pdf {
        Some(page_offsets.len())
    } else {
        None
    };

    let spans = chunking::chunk(
        &text,
        cfg.chunking.chunk_size_bytes,
        cfg.chunking.overlap_bytes,
    );

    tracing::info!(
        path = %path.display(),
        doc_type = doc_type.as_str(),
        total_size_bytes,
        total_size_chars,
        total_size_pages = ?total_size_pages,
        chunk_count = spans.len(),
        "ingest: extracted and chunked",
    );

    insert_all(
        db,
        cfg,
        http,
        &path,
        doc_type,
        &text,
        &spans,
        &page_offsets,
        total_size_bytes,
        total_size_chars,
        total_size_pages,
    )
    .await
}

fn detect_type(path: &Path) -> Result<DocType, IngestError> {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) {
        Some(ref e) if e == "txt" => Ok(DocType::Txt),
        Some(ref e) if e == "pdf" => Ok(DocType::Pdf),
        _ => Err(IngestError::UnknownExtension {
            path: path.to_path_buf(),
        }),
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_all(
    db: &Db,
    cfg: &Config,
    http: &reqwest::Client,
    path: &Path,
    doc_type: DocType,
    text: &str,
    spans: &[ChunkSpan],
    page_offsets: &[PageOffset],
    total_size_bytes: usize,
    total_size_chars: usize,
    total_size_pages: Option<usize>,
) -> Result<i64, IngestError> {
    let conn = &db.conn;

    conn.execute("BEGIN", ())
        .await
        .map_err(|source| IngestError::Db { op: "begin", source })?;

    let result = insert_all_inner(
        db,
        cfg,
        http,
        path,
        doc_type,
        text,
        spans,
        page_offsets,
        total_size_bytes,
        total_size_chars,
        total_size_pages,
    )
    .await;

    match result {
        Ok(id) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(|source| IngestError::Db {
                    op: "commit",
                    source,
                })?;
            Ok(id)
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_all_inner(
    db: &Db,
    cfg: &Config,
    http: &reqwest::Client,
    path: &Path,
    doc_type: DocType,
    text: &str,
    spans: &[ChunkSpan],
    page_offsets: &[PageOffset],
    total_size_bytes: usize,
    total_size_chars: usize,
    total_size_pages: Option<usize>,
) -> Result<i64, IngestError> {
    let conn = &db.conn;
    let path_str = path.to_string_lossy().to_string();

    let document_id = insert_document(
        conn,
        &path_str,
        doc_type,
        total_size_bytes,
        total_size_chars,
        total_size_pages,
    )
    .await?;

    if doc_type == DocType::Pdf {
        for p in page_offsets {
            conn.execute(
                "INSERT INTO document_page \
                    (document_id, page_number, byte_start, byte_end, char_start, char_end) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                Params::Positional(vec![
                    Value::Integer(document_id),
                    Value::Integer(p.page_number as i64),
                    Value::Integer(p.byte_start as i64),
                    Value::Integer(p.byte_end as i64),
                    Value::Integer(p.char_start as i64),
                    Value::Integer(p.char_end as i64),
                ]),
            )
            .await
            .map_err(|source| IngestError::Db {
                op: "insert_document_page",
                source,
            })?;
        }
    }

    for (i, span) in spans.iter().enumerate() {
        let chunk_text = &text[span.byte_start..span.byte_end];

        let (page_first, page_last) = if doc_type == DocType::Pdf && !page_offsets.is_empty() {
            let pf = pdf::page_for_byte_or_preceding(page_offsets, span.byte_start)
                .unwrap_or(page_offsets[0].page_number);
            let last_byte = span.byte_end.saturating_sub(1).max(span.byte_start);
            let pl = pdf::page_for_byte_or_preceding(page_offsets, last_byte)
                .unwrap_or(pf);
            (Some(pf as i64), Some(pl as i64))
        } else {
            (None, None)
        };

        let chunk_id = insert_chunk(
            conn,
            document_id,
            i as i64,
            chunk_text,
            span,
            page_first,
            page_last,
        )
        .await?;

        let embedding = ollama::embed(
            http,
            &cfg.ollama.url,
            &cfg.ollama.embedding_model,
            chunk_text,
        )
        .await
        .map_err(|source| IngestError::Embed {
            path: path.to_path_buf(),
            chunk_index: i,
            source,
        })?;

        if embedding.len() != db.embedding_dimensions {
            return Err(IngestError::EmbedDimMismatch {
                chunk_index: i,
                got: embedding.len(),
                expected: db.embedding_dimensions,
            });
        }

        let blob = vector::to_blob(&embedding);
        let insert_vec = format!(
            "INSERT INTO {} (chunk_id, embedding) VALUES (?, ?)",
            db.chunks_table
        );
        conn.execute(
            &insert_vec,
            Params::Positional(vec![Value::Integer(chunk_id), Value::Blob(blob)]),
        )
        .await
        .map_err(|source| IngestError::Db {
            op: "insert_embedding",
            source,
        })?;
    }

    Ok(document_id)
}

async fn insert_document(
    conn: &turso::Connection,
    path: &str,
    doc_type: DocType,
    total_size_bytes: usize,
    total_size_chars: usize,
    total_size_pages: Option<usize>,
) -> Result<i64, IngestError> {
    let pages_value = match total_size_pages {
        Some(n) => Value::Integer(n as i64),
        None => Value::Null,
    };

    let res = conn
        .execute(
            "INSERT INTO document \
                (path, doc_type, total_size_bytes, total_size_chars, total_size_pages) \
             VALUES (?, ?, ?, ?, ?)",
            Params::Positional(vec![
                Value::Text(path.to_string()),
                Value::Text(doc_type.as_str().to_string()),
                Value::Integer(total_size_bytes as i64),
                Value::Integer(total_size_chars as i64),
                pages_value,
            ]),
        )
        .await;

    match res {
        Ok(_) => {}
        Err(source) => {
            // UNIQUE(path) violation reads as the friendlier AlreadyIngested.
            let msg = source.to_string().to_ascii_lowercase();
            if msg.contains("unique") || msg.contains("constraint") {
                return Err(IngestError::AlreadyIngested {
                    path: PathBuf::from(path),
                });
            }
            return Err(IngestError::Db {
                op: "insert_document",
                source,
            });
        }
    }

    let mut rows = conn
        .query("SELECT last_insert_rowid()", ())
        .await
        .map_err(|source| IngestError::Db {
            op: "last_insert_rowid",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| IngestError::Db {
            op: "last_insert_rowid.next",
            source,
        })?
        .expect("last_insert_rowid returns one row");
    let id: i64 = row.get(0).map_err(|source| IngestError::Db {
        op: "last_insert_rowid.get",
        source,
    })?;
    Ok(id)
}

async fn insert_chunk(
    conn: &turso::Connection,
    document_id: i64,
    chunk_index: i64,
    text: &str,
    span: &ChunkSpan,
    page_first: Option<i64>,
    page_last: Option<i64>,
) -> Result<i64, IngestError> {
    let pf = page_first.map(Value::Integer).unwrap_or(Value::Null);
    let pl = page_last.map(Value::Integer).unwrap_or(Value::Null);

    conn.execute(
        "INSERT INTO document_chunk \
            (document_id, chunk_index, text, byte_start, byte_end, char_start, char_end, \
             page_first, page_last) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        Params::Positional(vec![
            Value::Integer(document_id),
            Value::Integer(chunk_index),
            Value::Text(text.to_string()),
            Value::Integer(span.byte_start as i64),
            Value::Integer(span.byte_end as i64),
            Value::Integer(span.char_start as i64),
            Value::Integer(span.char_end as i64),
            pf,
            pl,
        ]),
    )
    .await
    .map_err(|source| IngestError::Db {
        op: "insert_document_chunk",
        source,
    })?;

    let mut rows = conn
        .query("SELECT last_insert_rowid()", ())
        .await
        .map_err(|source| IngestError::Db {
            op: "last_insert_rowid",
            source,
        })?;
    let row = rows
        .next()
        .await
        .map_err(|source| IngestError::Db {
            op: "last_insert_rowid.next",
            source,
        })?
        .expect("last_insert_rowid returns one row");
    let id: i64 = row.get(0).map_err(|source| IngestError::Db {
        op: "last_insert_rowid.get",
        source,
    })?;
    Ok(id)
}
