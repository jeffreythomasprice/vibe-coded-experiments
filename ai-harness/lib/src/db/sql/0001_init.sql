-- Initial schema: agent configs, conversations, turns, messages, and the
-- tool_calls projection. See `lib::db`'s module doc for the invariants this
-- shape depends on.

CREATE TABLE agent_configs (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    -- The whole `shared::agent::AgentConfigInput`, verbatim. One JSON column
    -- rather than a column per field, for the same reason `messages.content`
    -- is one: `shared` owns the shape, serde is the only thing that has to
    -- agree with it, and a tool-selection list is not worth normalizing for
    -- a handful of rows. The columns beside it exist only so a row can be
    -- listed and grouped by provider/model without parsing the JSON.
    config_json TEXT NOT NULL,
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX agent_configs_name ON agent_configs(name);

CREATE TABLE conversations (
    id                INTEGER PRIMARY KEY,
    title             TEXT,
    -- Soft link: which saved agent this was started from. NULL once that
    -- agent config is deleted; the conversation stays runnable regardless,
    -- because `agent_config_json` below is the authoritative copy.
    agent_config_id   INTEGER REFERENCES agent_configs(id) ON DELETE SET NULL,
    -- The `AgentConfig` frozen as of when this conversation started. On
    -- purpose: `Agent::run_loop` rewrites `conversation.system` from the
    -- spec on every turn, so a live join to a mutable row would silently
    -- rewrite the premise of history that already ran. Re-pointing at a
    -- later edit of the agent is an explicit user action, not a side effect
    -- of editing that agent.
    agent_config_json TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    -- NULL until the first message. Drives the sidebar's ordering.
    last_message_at   TEXT
) STRICT;

CREATE INDEX conversations_recent ON conversations(last_message_at DESC, id DESC);

CREATE TABLE turns (
    id                 INTEGER PRIMARY KEY,
    conversation_id    INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- 1-based, per conversation.
    ordinal            INTEGER NOT NULL,
    state              TEXT NOT NULL
        CHECK (state IN ('running','awaiting_approval','done','max_steps','failed')),
    -- A serialized `shared::agent::AgentTurn`, present only while this turn
    -- has a resume point (state = 'running' or 'awaiting_approval'). NOT a
    -- record of the turn — a continuation token. `Agent::resume` re-validates
    -- that `turn.conversation`'s trailing assistant message still matches
    -- `pending`+`completed`, and carries `steps` forward (which decides the
    -- next `call_{step}_{ordinal}` ids and so must not restart), and
    -- `completed` holds tool results that already ran and are deliberately
    -- absent from `conversation`. Only a serde round-trip of the whole value
    -- can promise all of that back byte-identically; reconstructing it from
    -- `messages` rows cannot.
    turn_json          TEXT,
    -- A serialized `shared::llm::message::StopReason`, once state='done'.
    stop_reason        TEXT,
    steps              INTEGER NOT NULL DEFAULT 0,
    input_tokens       INTEGER NOT NULL DEFAULT 0,
    output_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens  INTEGER,
    cache_write_tokens INTEGER,
    -- A serialized `shared::error::ErrorReport`, once state='failed'.
    error_json         TEXT,
    started_at         TEXT NOT NULL,
    finished_at        TEXT,
    UNIQUE (conversation_id, ordinal),
    CHECK (state <> 'awaiting_approval' OR turn_json IS NOT NULL),
    CHECK (state IN ('running','awaiting_approval') OR turn_json IS NULL)
) STRICT;

-- At most one un-settled turn per conversation. The durable backstop for
-- `lib::service`'s in-memory per-conversation lock: the lock exists so the
-- common case is a clean typed error and so the guard spans the network call
-- that no transaction can cover; this exists so a bug in the lock cannot
-- interleave two turns into one history.
CREATE UNIQUE INDEX turns_one_open_per_conversation
    ON turns(conversation_id) WHERE state IN ('running','awaiting_approval');

CREATE INDEX turns_by_conversation ON turns(conversation_id, ordinal);

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- 0-based position in `Conversation.messages`. The authoritative order —
    -- never `id`, never a timestamp.
    seq             INTEGER NOT NULL,
    turn_id         INTEGER REFERENCES turns(id) ON DELETE SET NULL,
    role            TEXT NOT NULL CHECK (role IN ('user','assistant')),
    -- `serde_json::to_string` of `Vec<ContentBlock>`, verbatim. Not exploded
    -- into a blocks table: `ContentBlock` is `#[serde(tag="type")]` with a
    -- `#[serde(other)] Unknown` fallback and an opaque `Thinking.signature`
    -- that must replay byte-for-byte. The JSON *is* the value; normalizing
    -- it on the way in or out risks changing what gets replayed to the
    -- provider.
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (conversation_id, seq)
) STRICT;

CREATE INDEX messages_by_turn ON messages(turn_id);

-- A projection over `messages.content` and `turns.turn_json`, never a source
-- of truth — nothing may infer control-flow state (such as "this call is
-- still awaiting approval") from a row here; that's `ConversationView.pending`'s
-- job alone. Rebuilt wholesale per turn by `rebuild_tool_calls`; the columns
-- that are not derivable, and so must survive every rebuild untouched, are
-- `decision` (because `Agent::resume`'s denial placeholder is shape-identical
-- to a tool that failed on its own) and, since migration 0005,
-- `decided_by`/`decision_reason`/`decided_at` for the same reason.
CREATE TABLE tool_calls (
    turn_id         INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    -- `call_{step}_{ordinal}`, assigned by `lib::agent::rewrite_tool_use_ids`.
    -- Unique within a turn and ONLY within a turn — the step counter restarts
    -- every turn, so `call_1_0` recurs in every turn of a conversation. This
    -- key is exactly as wide as that guarantee.
    tool_use_id     TEXT NOT NULL,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- Both NULL while the turn is suspended: a suspended turn's messages are
    -- deliberately not in `messages` yet (see this file's `turns` doc).
    message_id      INTEGER REFERENCES messages(id) ON DELETE CASCADE,
    block_index     INTEGER,
    name            TEXT NOT NULL,
    input_json      TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('pending','ok','error')),
    -- 'approve'/'deny' once the user has ruled; NULL means never gated.
    -- Written once, by the resume path, and left alone by every rebuild.
    decision        TEXT CHECK (decision IN ('approve','deny')),
    -- The whole `ContentBlock::ToolResult`, once there is one.
    result_json     TEXT,
    created_at      TEXT NOT NULL,
    resolved_at     TEXT,
    PRIMARY KEY (turn_id, tool_use_id),
    CHECK ((state = 'pending') = (result_json IS NULL))
) STRICT;

CREATE INDEX tool_calls_pending ON tool_calls(conversation_id) WHERE state = 'pending';
CREATE INDEX tool_calls_by_name ON tool_calls(name, state);
