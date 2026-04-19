-- Cache of embedding-vector lengths probed at server startup. Avoids a probe
-- HTTP call to the embeddings provider on every boot once a model has been
-- seen at least once. `model` is the provider-side model identifier as it
-- appears in the loaded config (e.g. "qwen3-embedding:8b").

CREATE TABLE embedding_model_dimensions (
    model      TEXT    PRIMARY KEY,
    dimensions INTEGER NOT NULL,
    probed_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
