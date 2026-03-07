# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A fully local RAG (Retrieval-Augmented Generation) system: ingest PDFs/text files, chunk and embed them, store in PostgreSQL+pgvector, then query via semantic search with optional LLM synthesis or agentic tool-calling mode. All inference runs through Ollama locally.

## Commands

```bash
# Setup
docker compose up -d                                    # Start PostgreSQL+pgvector and Ollama
docker exec rag-ollama ollama pull nomic-embed-text     # Pull embedding model
docker exec rag-ollama ollama pull glm-4.7-flash        # Pull chat model
uv sync                                                 # Install Python dependencies

# Usage (all commands via cli.py)
uv run cli.py ingest <file> -t key=value                # Ingest a single file
uv run cli.py ingest-dir <dir> --ext .pdf --ext .txt    # Ingest a directory
uv run cli.py query "question" --raw                    # Raw vector search (no LLM)
uv run cli.py query "question"                          # Vector search + LLM synthesis
uv run cli.py agent "question"                          # Agentic mode (LLM decides searches)

# Database access
docker exec -it rag-postgres psql -U raguser -d ragdb
```

## Architecture

Single-directory Python project (no packages/src layout). Python 3.14, managed with `uv`.

**Data flow:** `cli.py` (Click CLI) → `ingest.py` / `query.py` / `agent.py` → `db.py` + `embeddings.py` + `text.py`

| Module | Role |
|--------|------|
| `config.py` | Loads `.env`, validates required vars, auto-pulls missing Ollama models on import |
| `text.py` | PDF extraction (PyMuPDF) and recursive character text chunking |
| `embeddings.py` | LiteLLM wrapper for Ollama embedding calls |
| `db.py` | psycopg + pgvector: insert documents/tags/chunks, cosine similarity search, context window fetch |
| `ingest.py` | Pipeline: extract → chunk → embed (batched) → store |
| `query.py` | `retrieve()` (vector search + context expansion) and `ask()` (retrieve + LLM synthesis) |
| `agent.py` | Agentic loop: LLM with `search_documents` tool, up to 10 iterations |
| `cli.py` | Click CLI with `ingest`, `ingest-dir`, `query`, `agent` commands |

**Key design details:**
- `config.py` is imported at module level by most files; it runs validation and model pulls on first import
- Context window expansion (`CONTEXT_WINDOW` env var): query results are expanded with surrounding chunks from the same document, with overlapping ranges merged and deduplicated
- Tag filtering uses AND logic via EXISTS subqueries in SQL
- The agent uses OpenAI function-calling schema routed through LiteLLM to Ollama

## Configuration

All config via `.env` file (see `.env.template`). Required vars: `OLLAMA_BASE_URL`, `EMBED_MODEL`, `CHAT_MODEL`, `DB_DSN`, `CHUNK_SIZE`, `CHUNK_OVERLAP`, `CONTEXT_WINDOW`.

## Database schema

Defined in `init.sql`, auto-applied by Docker on first run. Three tables: `documents`, `document_tags` (key=value pairs), `chunks` (text + 768-dim vector with HNSW index).
