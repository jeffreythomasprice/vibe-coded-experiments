CREATE EXTENSION IF NOT EXISTS vector;

-- Documents table: tracks ingested files
CREATE TABLE documents (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Tags table: arbitrary string tags associated with a document
CREATE TABLE document_tags (
    id          SERIAL PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    UNIQUE (document_id, tag)
);
CREATE INDEX idx_tags_tag ON document_tags (tag);
CREATE INDEX idx_tags_doc ON document_tags (document_id);

-- Cache table: key=value pairs for caching (e.g. embedding dimensions)
CREATE TABLE cache (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
