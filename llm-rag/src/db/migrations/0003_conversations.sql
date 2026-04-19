-- Chat conversations: header, message log (including tool-use rows), and a
-- many-to-many link to the shared `tags` table. UUIDs (TEXT) are minted by
-- the handler layer; timestamps stay ISO-8601 strings via datetime('now').

CREATE TABLE conversations (
    id         TEXT    PRIMARY KEY,
    title      TEXT,
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX conversations_updated_at_idx ON conversations (updated_at DESC);

CREATE TABLE conversation_tags (
    conversation_id TEXT    NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    tag_id          INTEGER NOT NULL REFERENCES tags(id)          ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, tag_id)
);

CREATE INDEX conversation_tags_tag_idx ON conversation_tags (tag_id);

CREATE TABLE conversation_messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT    NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- role ∈ {system,user,assistant,tool_use,tool}; enforced in MessageRole.
    role            TEXT    NOT NULL,
    content         TEXT    NOT NULL DEFAULT '',
    metadata        TEXT,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX conversation_messages_conv_idx ON conversation_messages (conversation_id, id);
