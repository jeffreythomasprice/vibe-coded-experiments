//! Document ingest: read a text file from disk, split it into overlapping
//! chunks, embed each chunk, and persist the document + chunks + tags in a
//! single database transaction.
//!
//! Network calls (embeddings) happen BEFORE opening the DB transaction so a
//! slow or flaky provider doesn't hold a write lock. Only the SQLite writes
//! live inside `BEGIN`/`COMMIT`; on any persistence failure we `ROLLBACK` and
//! the database is left untouched.

pub mod chunking;
pub mod error;
pub mod read;

use std::path::Path;
use std::sync::Arc;

use crate::db::{ChunkRange, Dal};
use crate::llm::EmbeddingProvider;

pub use chunking::{Chunk, chunk_sizes_for};
pub use error::IngestError;
pub use read::MAX_FILE_BYTES;

/// Progress event emitted as each chunk is persisted.
#[derive(Debug, Clone, Copy)]
pub struct ChunkProgress {
    /// 0-indexed chunk just persisted.
    pub index: usize,
    /// Total number of chunks in the document.
    pub total: usize,
}

/// Ingest a text file end-to-end. On success returns `(document_id, chunks)`.
///
/// `on_progress`, if present, is invoked once per persisted chunk so callers
/// can forward the progress over the wire (e.g. `DocumentCreateProgress`).
pub async fn ingest_document<F>(
    dal: &Dal,
    embeddings: &Arc<dyn EmbeddingProvider>,
    path: &Path,
    tags: &[String],
    mut on_progress: Option<F>,
) -> Result<(i64, usize), IngestError>
where
    F: FnMut(ChunkProgress),
{
    let text = read::read_text_file(path)?;
    let (chunk_chars, overlap_chars) = chunking::chunk_sizes_for(embeddings.max_input_tokens());
    let chunks = chunking::chunk(&text, chunk_chars, overlap_chars);
    if chunks.is_empty() {
        return Err(IngestError::NotTextFile {
            path: path.to_path_buf(),
            reason: "file is empty",
        });
    }

    let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
    let vectors = embeddings.embed_batch(&chunk_texts).await?;
    if vectors.len() != chunks.len() {
        return Err(IngestError::Embed(crate::llm::LlmError::Provider(
            anyhow::anyhow!(
                "embed_batch returned {} vectors for {} chunks",
                vectors.len(),
                chunks.len()
            ),
        )));
    }

    let path_str = path.to_string_lossy().into_owned();
    let total = chunks.len();

    dal.begin().await?;
    let outcome = persist(dal, &path_str, &chunks, &vectors, tags, &mut on_progress).await;
    match outcome {
        Ok(document_id) => {
            dal.commit().await?;
            Ok((document_id, total))
        }
        Err(err) => {
            // Best-effort rollback; if it fails, return the original error.
            let _ = dal.rollback().await;
            Err(err)
        }
    }
}

async fn persist<F>(
    dal: &Dal,
    path: &str,
    chunks: &[Chunk],
    vectors: &[Vec<f32>],
    tags: &[String],
    on_progress: &mut Option<F>,
) -> Result<i64, IngestError>
where
    F: FnMut(ChunkProgress),
{
    let document_id = dal.create_document(path).await?;
    let total = chunks.len();

    for (index, (chunk, embedding)) in chunks.iter().zip(vectors.iter()).enumerate() {
        dal.create_chunk(
            document_id,
            ChunkRange::Bytes {
                start: chunk.byte_start,
                end: chunk.byte_end,
            },
            &chunk.content,
            embedding,
        )
        .await?;
        if let Some(cb) = on_progress {
            cb(ChunkProgress { index, total });
        }
    }

    for tag in tags {
        dal.add_tag(document_id, tag).await?;
    }

    Ok(document_id)
}
