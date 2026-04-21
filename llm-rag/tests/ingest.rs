//! End-to-end ingest test using the `mock-llm` feature so we never touch the
//! network. Queues fixed-dimension vectors on a mock embeddings provider,
//! runs `ingest::ingest_document` against a real DB, and asserts on the
//! persisted state (document row, chunk count, tag-filtered search hits).

#![cfg(feature = "mock-llm")]

use std::path::PathBuf;
use std::sync::Arc;

use llm_rag::db::Dal;
use llm_rag::ingest;
use llm_rag::llm::EmbeddingProvider;
use llm_rag::llm::mock::MockProvider;
use tempfile::TempDir;

async fn open_dal(path: &std::path::Path, dims: usize) -> Dal {
    Dal::open(path, format!("mock-{dims}"), || async move { Ok(dims) })
        .await
        .unwrap()
}

#[tokio::test]
async fn ingest_persists_document_chunks_and_tags() {
    // Pick text long enough to span multiple chunks so we exercise the
    // overlap path. `ingest::chunking::chunk` is the single source of truth
    // for the chunk count — derive it here instead of hard-coding.
    const DIMS: usize = 4;
    let text: String = (0..(ingest::CHUNK_CHARS * 3))
        .map(|i| (b'a' + (i % 26) as u8) as char)
        .collect();
    let expected_chunks = ingest::chunking::chunk(&text).len();
    assert!(
        expected_chunks >= 2,
        "test fixture should produce multiple chunks"
    );

    let tmp_dir = TempDir::new().unwrap();
    let db_path: PathBuf = tmp_dir.path().join("ingest.db");
    let txt_path: PathBuf = tmp_dir.path().join("doc.txt");
    std::fs::write(&txt_path, &text).unwrap();

    let dal = open_dal(&db_path, DIMS).await;

    let mock = MockProvider::new();
    // Queue one distinct vector per chunk so each row is uniquely identifiable.
    for i in 0..expected_chunks {
        let mut v = vec![0.0f32; DIMS];
        v[i % DIMS] = 1.0;
        mock.push_embedding(v);
    }
    let embeddings: Arc<dyn EmbeddingProvider> = Arc::new(mock);

    let tags = vec!["alpha".to_string(), "beta".to_string()];
    let (doc_id, chunks) =
        ingest::ingest_document(&dal, &embeddings, &txt_path, &tags, None::<fn(_)>)
            .await
            .expect("ingest should succeed");
    assert_eq!(chunks, expected_chunks);

    // Document was written.
    let doc = dal.get_document(doc_id).await.unwrap().expect("doc row");
    assert_eq!(doc.path, txt_path.to_string_lossy());

    // Tags attached.
    let mut got_tags = dal.tags_for_document(doc_id).await.unwrap();
    got_tags.sort();
    assert_eq!(got_tags, vec!["alpha".to_string(), "beta".to_string()]);

    // Vector search, no tag filter, returns both chunks.
    let hits = dal
        .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &[])
        .await
        .unwrap();
    assert_eq!(hits.len(), expected_chunks);
    // Nearest hit is the first chunk.
    assert!(hits[0].distance <= hits[1].distance);

    // Tag filter: both tags → both chunks (the document has both).
    let hits = dal
        .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &["alpha", "beta"])
        .await
        .unwrap();
    assert_eq!(hits.len(), expected_chunks);

    // Tag filter: missing tag → zero chunks.
    let hits = dal
        .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &["nonexistent"])
        .await
        .unwrap();
    assert!(hits.is_empty());

    // list_documents sees the row.
    let docs = dal.list_documents().await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, doc_id);
}

#[tokio::test]
async fn ingest_rejects_binary_files() {
    let tmp_dir = TempDir::new().unwrap();
    let db_path: PathBuf = tmp_dir.path().join("ingest.db");
    let binary_path: PathBuf = tmp_dir.path().join("binary.bin");
    // NUL byte in the first 8 KB triggers the binary heuristic.
    std::fs::write(&binary_path, b"hello\0world").unwrap();

    let dal = open_dal(&db_path, 4).await;
    let embeddings: Arc<dyn EmbeddingProvider> = Arc::new(MockProvider::new());

    let err = ingest::ingest_document(&dal, &embeddings, &binary_path, &[], None::<fn(_)>)
        .await
        .expect_err("binary file should be rejected");
    assert!(matches!(err, ingest::IngestError::NotTextFile { .. }));

    // Nothing was written.
    assert!(dal.list_documents().await.unwrap().is_empty());
}
