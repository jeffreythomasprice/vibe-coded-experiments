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
