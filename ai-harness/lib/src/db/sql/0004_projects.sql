-- Projects: a name plus zero or more real directories, each read-only or
-- read-write, defining an agent's sandbox (see `lib::vfs`/`lib::sandbox`).
-- `dirs_json` is a serialized `Vec<shared::project::ProjectDir>` — one JSON
-- column rather than a child table, the same reason `agent_configs.config_json`
-- is one: `shared` owns the shape, and a handful of directories per project
-- isn't worth normalizing.
--
-- The default project (zero directories, no filesystem access) is never a
-- row here — see `shared::project`'s module doc, which mirrors how
-- `shared::theme::BuiltIn` is a compiled-in enum, never a `themes` row.

CREATE TABLE projects (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    dirs_json  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

-- Unique case-insensitively, like `themes_name`: a project name is what the
-- new-conversation picker shows, and two identical entries there is a trap.
-- The other half of "unique including the default project's reserved name"
-- can't be a SQL constraint (the default is never a row) — see
-- `shared::project::default_project_name_conflict`.
CREATE UNIQUE INDEX projects_name ON projects(name COLLATE NOCASE);

-- Which project a conversation runs under. Unlike `agent_config_id` (see
-- `agent_config_json` above), this is a SOFT link with **no** frozen JSON
-- twin: a project's directories are re-resolved live on every use, not
-- decoded from a copy taken at creation time. An agent config is a premise —
-- rewriting it would rewrite the premise of history that already ran. A
-- project is a live grant: removing a directory from it is the entire point
-- of the read-only/read-write toggle, and that removal must take effect for
-- every conversation open on the project immediately, not just new ones.
--
-- NULL means "the default project" — both because none was ever chosen and
-- because a chosen one was since deleted (`ON DELETE SET NULL`). Those two
-- cases are indistinguishable here on purpose: both resolve to the same
-- empty, safe virtual filesystem, so the ambiguity between them is harmless
-- by construction. See `lib::service::projects::resolve`/`conversation_mounts`.
ALTER TABLE conversations ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL;
