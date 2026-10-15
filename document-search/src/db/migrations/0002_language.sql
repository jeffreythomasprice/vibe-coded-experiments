-- Per-document detected language and per-summary output language. Existing
-- summaries were all produced by the English-only prompt, so they backfill
-- to 'en'. The summary indexes gain `language` so per-language tree builds
-- (dedup lookups, level loads) stay index-backed.
ALTER TABLE document ADD COLUMN language TEXT;

ALTER TABLE document_summary ADD COLUMN language TEXT NOT NULL DEFAULT 'en';

DROP INDEX document_summary_doc_level_idx;

DROP INDEX document_summary_doc_range_idx;

CREATE INDEX document_summary_doc_level_idx
    ON document_summary (document_id, language, level);

CREATE INDEX document_summary_doc_range_idx
    ON document_summary (document_id, language, level, byte_start, byte_end);
