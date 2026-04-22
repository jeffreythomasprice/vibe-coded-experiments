-- Cache the embedding model's advertised max input-token context length
-- alongside its vector length. Populated at startup by probing the provider
-- (e.g. Ollama's /api/show). Nullable: older rows pre-date this column and a
-- provider without a reliable context-length signal leaves it NULL.

ALTER TABLE embedding_model_dimensions
    ADD COLUMN max_input_tokens INTEGER;
