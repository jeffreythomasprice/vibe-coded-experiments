"""Generate embeddings via LiteLLM → Ollama."""

from __future__ import annotations

import litellm

from config import EMBED_MODEL, OLLAMA_BASE_URL


def embed_texts(texts: list[str]) -> list[list[float]]:
    """
    Return one embedding vector per input text.

    Uses LiteLLM which routes to Ollama's /api/embeddings endpoint.
    Batches automatically if the provider supports it.
    """
    response = litellm.embedding(
        model=EMBED_MODEL,
        input=texts,
        api_base=OLLAMA_BASE_URL,
    )
    # response.data is a list of EmbeddingObject with .embedding
    return [item["embedding"] for item in response.data]


def embed_single(text: str) -> list[float]:
    return embed_texts([text])[0]
