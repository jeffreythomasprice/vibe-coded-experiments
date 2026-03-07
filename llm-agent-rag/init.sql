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

-- Cache table: key=value pairs for caching (e.g. embedding dimensions)
CREATE TABLE cache (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
