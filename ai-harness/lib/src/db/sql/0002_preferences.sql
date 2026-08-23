-- A generic key-value store for small, user-set UI state — the first entry
-- is the preferred theme (`shared::theme::THEME_PREFERENCE_KEY`), but the
-- table doesn't know that: `value` is opaque TEXT, and each key's caller owns
-- its own encoding, the same way `agent_configs.config_json` leaves its shape
-- to `shared`. Distinct from `config.toml`, which is immutable deployment
-- config read once at startup — this is state a running app writes.

CREATE TABLE preferences (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;
