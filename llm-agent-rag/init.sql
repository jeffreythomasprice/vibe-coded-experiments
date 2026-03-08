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

-- Conversations table: agent chat conversations
CREATE TABLE conversations (
    id         SERIAL PRIMARY KEY,
    title      TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE conversation_messages (
    id              SERIAL PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    tool_info       JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_conv_messages_conv ON conversation_messages (conversation_id, created_at);
