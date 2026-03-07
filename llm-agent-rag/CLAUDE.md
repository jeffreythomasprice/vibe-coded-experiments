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

# Usage (all commands via cli.ts)
bun run src/cli.ts ingest <file> -t key=value               # Ingest a single file
bun run src/cli.ts ingest-dir <dir> --ext .pdf --ext .txt   # Ingest a directory
bun run src/cli.ts query "question" --raw                   # Raw vector search (no LLM)
bun run src/cli.ts query "question"                         # Vector search + LLM synthesis
bun run src/cli.ts agent "question"                         # Agentic mode (LLM decides searches)
bun run src/cli.ts documents                                # List ingested documents
bun run src/cli.ts tags                                     # List all tags
bun run src/cli.ts find -t key=value                        # Find documents by tags

# Database access
docker exec -it rag-postgres psql -U raguser -d ragdb
```

## Architecture

Single-directory TypeScript project running on Bun. Uses Vercel AI SDK with `ollama-ai-provider-v2` for LLM/embedding calls, and `@mastra/rag` for document chunking.

**Data flow:** `cli.ts` (Commander CLI) → `ingest.ts` / `query.ts` / `agent.ts` → `db.ts` + `embeddings.ts` + `text.ts`

| Module | Role |
|--------|------|
| `config.ts` | Reads `Bun.env`, validates required vars, auto-pulls missing Ollama models via streaming fetch |
| `text.ts` | PDF extraction (`pdf-parse`) and text chunking via Mastra `MDocument` recursive strategy |
| `embeddings.ts` | Vercel AI SDK `embedMany`/`embed` with `ollama-ai-provider-v2` for Ollama embedding calls |
| `db.ts` | `postgres` (porsager): insert documents/tags/chunks, cosine similarity search, context window fetch |
| `ingest.ts` | Pipeline: extract → chunk (Mastra MDocument) → embed (batched) → store |
| `query.ts` | `retrieve()` (vector search + context expansion) and `ask()` (retrieve + LLM synthesis via `generateText`) |
| `agent.ts` | Vercel AI SDK `generateText` with `tool()` and `stepCountIs(10)` for agentic tool-calling loop |
| `cli.ts` | Commander CLI with `ingest`, `ingest-dir`, `query`, `agent`, `documents`, `tags`, `find` commands |

**Key design details:**
- `initConfig()` is called via Commander `preAction` hook before any command runs
- Context window expansion (`CONTEXT_WINDOW` env var): query results are expanded with surrounding chunks from the same document, with overlapping ranges merged and deduplicated
- Tag filtering uses AND logic via EXISTS subqueries in SQL
- The agent uses Vercel AI SDK `tool()` with Zod schemas, routed through `ollama-ai-provider-v2` to Ollama

## Configuration

All config via `.env` file (see `.env.template`). Required vars: `OLLAMA_BASE_URL`, `EMBED_MODEL`, `CHAT_MODEL`, `DB_DSN`, `CHUNK_SIZE`, `CHUNK_OVERLAP`, `CONTEXT_WINDOW`.

Model names use bare Ollama model IDs (e.g. `nomic-embed-text`, `glm-4.7-flash`) without provider prefixes.

## Database schema

Defined in `init.sql`, auto-applied by Docker on first run. Three tables: `documents`, `document_tags` (key=value pairs), `chunks` (text + 768-dim vector with HNSW index).
