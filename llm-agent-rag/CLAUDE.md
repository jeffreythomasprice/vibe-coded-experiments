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
bun install                                             # Install dependencies

# Usage (all commands via index.ts)
bun run packages/app/src/index.ts ingest <file> -t key=value               # Ingest a single file
bun run packages/app/src/index.ts ingest-dir <dir> --ext .pdf --ext .txt   # Ingest a directory
bun run packages/app/src/index.ts query "question" --raw                   # Raw vector search (no LLM)
bun run packages/app/src/index.ts query "question"                         # Vector search + LLM synthesis
bun run packages/app/src/index.ts agent "question"                         # Agentic mode (LLM decides searches)
bun run packages/app/src/index.ts documents                                # List ingested documents
bun run packages/app/src/index.ts tags                                     # List all tags
bun run packages/app/src/index.ts find -t key=value                        # Find documents by tags
bun run packages/app/src/index.ts serve                                    # Start HTTP API server

# Database access
docker exec -it rag-postgres psql -U raguser -d ragdb
```

## Architecture

Bun workspace monorepo with two packages:
- `@rag/shared` (`packages/shared/`) — shared TypeScript interfaces (types-only, no runtime deps)
- `@rag/app` (`packages/app/`) — all backend code: CLI, HTTP server, RAG logic

Uses Vercel AI SDK with `ollama-ai-provider-v2` for LLM/embedding calls, and `@mastra/rag` for document chunking.

**Data flow:** `index.ts` (Commander CLI) → `ingest.ts` / `query.ts` / `agent.ts` → `db.ts` + `embeddings.ts` + `text.ts`

| Module (packages/app/src/) | Role |
|--------|------|
| `config.ts` | Reads `Bun.env`, validates required vars, auto-pulls missing Ollama models, detects and caches embedding dimension, creates dynamic chunks table |
| `text.ts` | PDF extraction (`pdf-parse`) and text chunking via Mastra `MDocument` recursive strategy |
| `embeddings.ts` | Vercel AI SDK `embedMany`/`embed` with `ollama-ai-provider-v2` for Ollama embedding calls |
| `db.ts` | `postgres` (porsager): dynamic `chunks_{dim}` tables with provider/model columns, cache helpers, cosine similarity search, context window fetch |
| `ingest.ts` | Pipeline: extract → chunk (Mastra MDocument) → embed (batched) → store |
| `query.ts` | `retrieve()` (vector search + context expansion) and `ask()` (retrieve + LLM synthesis via `generateText`) |
| `agent.ts` | Vercel AI SDK `generateText` with `tool()` and `stepCountIs(10)` for agentic tool-calling loop |
| `index.ts` | Commander CLI with `ingest`, `ingest-dir`, `query`, `agent`, `documents`, `tags`, `find`, `serve` commands |
| `server.ts` | Koa HTTP API server wrapping CLI functionality |

**HTTP API endpoints** (served by `server.ts` via `serve` command):

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/ingest` | Ingest a file or directory (body: `{ path, tags?, extensions? }`) |
| POST | `/api/query` | Raw vector search (body: `{ query, top_k?, tags? }`) |
| POST | `/api/ask` | Vector search + LLM synthesis (body: `{ query, top_k?, tags? }`) |
| POST | `/api/agent` | Agentic mode (body: `{ message, system_prompt? }`) |
| GET | `/api/documents` | List all ingested documents |
| GET | `/api/tags` | List all tags |
| POST | `/api/documents/find` | Find documents by tags (body: `{ tags }`) |

| Module (packages/shared/src/) | Role |
|--------|------|
| `types.ts` | Shared interfaces: `ChunkResult`, `Document`, `Tag`, `DocumentSummary`, `IngestResult`, `AskSource`, `AskResponse`, API request types |
| `index.ts` | Barrel export |

**Key design details:**
- `initConfig()` is called via Commander `preAction` hook before any command runs
- Context window expansion (`CONTEXT_WINDOW` env var): query results are expanded with surrounding chunks from the same document, with overlapping ranges merged and deduplicated
- Tag filtering uses AND logic via EXISTS subqueries in SQL
- The agent uses Vercel AI SDK `tool()` with Zod schemas, routed through `ollama-ai-provider-v2` to Ollama

## Configuration

All config via `.env` file (see `.env.template`). Required vars: `OLLAMA_BASE_URL`, `EMBED_PROVIDER`, `EMBED_MODEL`, `CHAT_MODEL`, `DB_DSN`, `CHUNK_SIZE`, `CHUNK_OVERLAP`, `CONTEXT_WINDOW`. Optional vars: `PORT` (default `8001`), `BIND_ADDRESS` (default `127.0.0.1`).

Model names use bare Ollama model IDs (e.g. `nomic-embed-text`, `glm-4.7-flash`) without provider prefixes.

## GPU Support (NVIDIA)

The `docker-compose.yml` includes NVIDIA GPU passthrough for Ollama. If you get `could not select device driver "nvidia"`, install the NVIDIA Container Toolkit. If no GPU is available, comment out the `deploy` section in `docker-compose.yml` to run on CPU.

## Database schema

Defined in `init.sql`, auto-applied by Docker on first run. Tables: `documents`, `document_tags` (key=value pairs), `cache` (key=value for caching e.g. embedding dimensions). Chunks are stored in dynamically created `chunks_{dim}` tables (e.g. `chunks_768`) with `embed_provider` and `embed_model` columns, created at startup based on the detected embedding dimension.
