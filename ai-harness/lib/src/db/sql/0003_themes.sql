-- User-authored themes. `theme_json` is a serialized `shared::theme::Theme`
-- (the whole 10-field palette) — this table doesn't know its shape, the same
-- way `agent_configs.config_json` leaves its shape to `shared`.
--
-- Names are unique case-insensitively, matching the settings page's rule
-- that a theme's name must be unique across every theme, built in and
-- user-provided; the built-in half of that rule can't be expressed as a SQL
-- constraint (built-ins never have a row here) and is enforced in
-- `shared::theme::built_in_name_conflict` instead.

CREATE TABLE themes (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    theme_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX themes_name ON themes(name COLLATE NOCASE);
