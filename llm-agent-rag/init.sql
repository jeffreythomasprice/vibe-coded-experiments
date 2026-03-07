CREATE EXTENSION IF NOT EXISTS vector;

-- Documents table: tracks ingested files
CREATE TABLE documents (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Tags table: key=value pairs associated with a document
CREATE TABLE document_tags (
    id          SERIAL PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL
);
CREATE INDEX idx_tags_kv ON document_tags (key, value);
CREATE INDEX idx_tags_doc ON document_tags (document_id);

-- Chunks table: text chunks with embeddings
CREATE TABLE chunks (
    id          SERIAL PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content     TEXT NOT NULL,
    embedding   vector(768) -- nomic-embed-text dimension
);
CREATE INDEX idx_chunks_doc ON chunks (document_id);

-- HNSW index for fast approximate nearest-neighbor search
CREATE INDEX idx_chunks_embedding ON chunks USING hnsw (embedding vector_cosine_ops);
