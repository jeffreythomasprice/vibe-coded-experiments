CREATE TABLE document (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    path              TEXT    NOT NULL UNIQUE,
    doc_type          TEXT    NOT NULL,
    total_size_bytes  INTEGER NOT NULL,
    total_size_chars  INTEGER NOT NULL,
    total_size_pages  INTEGER,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE document_chunk (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id  INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,
    text         TEXT    NOT NULL,
    byte_start   INTEGER NOT NULL,
    byte_end     INTEGER NOT NULL,
    char_start   INTEGER NOT NULL,
    char_end     INTEGER NOT NULL,
    page_first   INTEGER,
    page_last    INTEGER,
    UNIQUE (document_id, chunk_index)
);

CREATE INDEX document_chunk_doc_idx  ON document_chunk (document_id);
CREATE INDEX document_chunk_doc_byte ON document_chunk (document_id, byte_start);

CREATE TABLE document_page (
    document_id  INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    page_number  INTEGER NOT NULL,
    byte_start   INTEGER NOT NULL,
    byte_end     INTEGER NOT NULL,
    char_start   INTEGER NOT NULL,
    char_end     INTEGER NOT NULL,
    PRIMARY KEY (document_id, page_number)
);

CREATE TABLE embedding_model_dimensions (
    model      TEXT    PRIMARY KEY,
    dimensions INTEGER NOT NULL,
    probed_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE document_tag (
    document_id  INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    tag          TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (document_id, tag)
);

CREATE INDEX document_tag_tag_idx ON document_tag (tag);

CREATE TABLE document_summary (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id   INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    parent_id     INTEGER          REFERENCES document_summary(id) ON DELETE CASCADE,
    level         INTEGER NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    page_first    INTEGER,
    page_last     INTEGER,
    text          TEXT    NOT NULL,
    content_hash  TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX document_summary_doc_level_idx
    ON document_summary (document_id, level);

CREATE INDEX document_summary_doc_range_idx
    ON document_summary (document_id, level, byte_start, byte_end);

CREATE INDEX document_summary_parent_idx
    ON document_summary (parent_id);

CREATE INDEX document_summary_hash_idx
    ON document_summary (document_id, level, content_hash);
