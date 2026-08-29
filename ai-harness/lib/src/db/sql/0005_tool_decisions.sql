-- Provenance for a gated tool call's `decision` — who resolved it (a person,
-- through the approval UI, or an automatic policy) and when. `decision`
-- itself already exists (see `0001_init.sql`'s `tool_calls`); these three
-- columns answer "who/why/when", not "what".
--
-- `decided_by`/`decision_reason`/`decided_at` are written **only** by
-- `lib::db::turns::settle` (via `write_decisions`), sourced from
-- `shared::agent::AgentTurn::decisions` — never by `reopen`, which keeps
-- writing only `decision`, the moment the user answers, exactly as it
-- already did before this migration. That keeps a single writer for these
-- three columns: a crash between `reopen` and the next `settle` leaves
-- `decision` set with these three still NULL, visible and self-healing once
-- the turn is driven again, never contradictory. `settle` also (re)writes
-- `decision` itself alongside these three — a *policy* decision never goes
-- through `reopen` at all (there's no suspend/resume for a call the policy
-- resolved on its own), so `settle` is that call's only writer for
-- `decision`, not just for the columns below.
--
-- Existing rows backfill NULL, which already means "never gated" for
-- `decision` — the same reading applies here.
ALTER TABLE tool_calls ADD COLUMN decided_by TEXT
    CHECK (decided_by IN ('user', 'policy'));
ALTER TABLE tool_calls ADD COLUMN decision_reason TEXT;
ALTER TABLE tool_calls ADD COLUMN decided_at TEXT;
