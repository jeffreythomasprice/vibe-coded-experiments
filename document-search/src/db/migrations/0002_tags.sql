CREATE TABLE document_tag (
    document_id  INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    tag          TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (document_id, tag)
);

CREATE INDEX document_tag_tag_idx ON document_tag (tag);
