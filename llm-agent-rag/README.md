# Local RAG System

A fully local RAG (Retrieval-Augmented Generation) system using **Ollama** for LLM inference and embeddings, **PostgreSQL + pgvector** for vector storage, and the **Vercel AI SDK** with `ollama-ai-provider-v2` as the unified SDK layer.

## Architecture

```
┌──────────┐     ┌──────────────┐     ┌──────────────────────┐
│  CLI     │────▶│  Ingest      │────▶│  PostgreSQL + pgvector│
│          │     │  Pipeline    │     │  • documents          │
│  ingest  │     │  extract     │     │  • document_tags      │
│  query   │     │  chunk       │     │  • chunks (+ vectors) │
│  agent   │     │  embed       │     └──────────┬───────────┘
│  docs    │     │              │                │
│  tags    │     │              │                │
│  find    │     │              │                │
└──────────┘     └──────┬───────┘                │
                        │                         │
                 ┌──────▼───────┐                 │
                 │  Ollama      │◀────────────────┘
                 │  • embeddings│      (query path)
                 │  • chat/tools│
                 └──────────────┘
```

### Components

| Component | Choice | Why |
|-----------|--------|-----|
| Language | TypeScript (Bun) | Fast runtime, native TypeScript support, built-in .env loading |
| LLM SDK | Vercel AI SDK + `ollama-ai-provider-v2` | Unified API for embeddings and chat, supports tool calling |
| Chunking | `@mastra/rag` | MDocument recursive character splitter with configurable boundaries |
| Database | PostgreSQL + pgvector | Mature, single DB for metadata + vectors, great Docker support, HNSW indexing |
| Embedding model | `nomic-embed-text` | 768-dim, 8192-token context, solid quality, runs well on CPU |
| Chat model | `glm-4.7-flash` | Good tool-calling support, strong general reasoning |
| PDF extraction | `pdf-parse` | Fast, reliable PDF text extraction |

## GPU Support (NVIDIA)

The `docker-compose.yml` includes an NVIDIA GPU passthrough config for Ollama. If you see this error when starting the container:

```
Error response from daemon: could not select device driver "nvidia" with capabilities: [[gpu]]
```

You need to install the **NVIDIA Container Toolkit**:

```bash
# Add the NVIDIA container toolkit repository
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
  sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
  sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list

# Install
sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit

# Configure Docker and restart
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

Verify with:

```bash
docker run --rm --gpus all nvidia/cuda:12.0.0-base-ubuntu22.04 nvidia-smi
```

If you don't have an NVIDIA GPU, comment out the `deploy` section in `docker-compose.yml` to run Ollama on CPU only.

## Quick Start

### 1. Start infrastructure

```bash
docker compose up -d
```

This launches PostgreSQL (with pgvector) and Ollama. The `init.sql` script creates the schema automatically.

### 2. Pull Ollama models

```bash
# If using the dockerized Ollama:
docker exec rag-ollama ollama pull nomic-embed-text
docker exec rag-ollama ollama pull glm-4.7-flash

# If using a local Ollama installation, pull directly:
# ollama pull nomic-embed-text
# ollama pull glm-4.7-flash
```

### 3. Install dependencies

```bash
bun install
```

### 4. Configure (optional)

Copy the template and edit as needed:

```bash
cp .env.template .env
```

The `.env` file contains all settings with sensible defaults:

```
OLLAMA_BASE_URL=http://localhost:11434
EMBED_PROVIDER=ollama
EMBED_MODEL=nomic-embed-text
CHAT_MODEL=glm-4.7-flash
DB_DSN=postgresql://raguser:ragpass@localhost:5432/ragdb
CHUNK_SIZE=1000
CHUNK_OVERLAP=200
CONTEXT_WINDOW=2
```

The `.env` file is gitignored so your local settings won't be committed. Bun loads `.env` automatically.

If you're running Ollama in Docker (as in the compose file), use port `11434` (the host-mapped port). If you already have Ollama running locally on `11434`, either stop it or change the compose port mapping.

## Usage

### Ingest a file

```bash
# Plain text
bun run packages/app/src/index.ts ingest notes.txt -t project=alpha -t author=jeff

# PDF
bun run packages/app/src/index.ts ingest report.pdf -t project=beta -t type=quarterly

# Entire directory
bun run packages/app/src/index.ts ingest-dir ./documents/ -t project=alpha --ext .pdf --ext .txt
```

The `filename` tag is always added automatically. Additional tags are arbitrary key=value pairs.

### Query (raw vector search)

```bash
# Semantic search — returns matching chunks
bun run packages/app/src/index.ts query "What were the Q3 revenue figures?" --raw

# Filter by tags
bun run packages/app/src/index.ts query "deployment architecture" --raw -t project=alpha

# Adjust result count
bun run packages/app/src/index.ts query "error handling patterns" --raw -k 10
```

### Query (with LLM synthesis)

```bash
# The LLM reads retrieved chunks and produces a grounded answer
bun run packages/app/src/index.ts query "Summarize the key findings from the quarterly report"

# With tag filters
bun run packages/app/src/index.ts query "What risks were identified?" -t type=quarterly
```

### Agent mode

The agent autonomously decides when and how to search. It can make multiple searches with different queries and filters before answering.

```bash
bun run packages/app/src/index.ts agent "Compare the deployment approaches described in the alpha and beta project docs"
```

### HTTP API server

```bash
# Start the API server (default: http://127.0.0.1:8001)
bun run packages/app/src/index.ts serve

# Configure host/port via environment variables
PORT=3000 BIND_ADDRESS=0.0.0.0 bun run packages/app/src/index.ts serve
```

The server exposes the same functionality as the CLI over HTTP:

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/ingest` | Ingest a file or directory (`{ path, tags?, extensions? }`) |
| POST | `/api/ingest/upload` | Upload and ingest a file (multipart `file` + optional `tags` JSON string) |
| POST | `/api/query` | Raw vector search (`{ query, top_k?, tags? }`) |
| POST | `/api/ask` | Vector search + LLM synthesis (`{ query, top_k?, tags? }`) |
| POST | `/api/agent` | Agentic mode (`{ message, system_prompt? }`) |
| GET | `/api/documents` | List all ingested documents |
| DELETE | `/api/documents/:id` | Delete a document and all its chunks |
| GET | `/api/tags` | List all tags |
| POST | `/api/documents/find` | Find documents by tags (`{ tags }`) |

### Web frontend

A React UI for browsing documents, uploading files, and querying the RAG system.

```bash
# Start the API server in one terminal
bun run packages/app/src/index.ts serve

# Start the Vite dev server in another terminal
bun run --cwd packages/frontend dev
```

Open http://localhost:5173 in your browser. The frontend connects to the API server at the URL defined in `packages/frontend/.env` (`VITE_API_URL`, defaults to `http://127.0.0.1:8001`).

**Documents tab** — View all ingested documents, filter by tag, upload new files with tags, and delete documents.

**Query tab** — Three modes:
- **Search** — raw vector search returning matching chunks with similarity scores
- **Ask** — vector search + LLM synthesis producing an answer with sources
- **Agent** — agentic mode where the LLM autonomously decides how to search

To build for production:

```bash
bun run --cwd packages/frontend build
```

Output goes to `packages/frontend/dist/`.

### Browse & filter

```bash
# List all ingested documents with their tags
bun run packages/app/src/index.ts documents

# List all tags and how many documents use each one
bun run packages/app/src/index.ts tags

# Find documents matching specific tags (AND logic)
bun run packages/app/src/index.ts find -t project=alpha
bun run packages/app/src/index.ts find -t project=alpha -t type=quarterly
```

## Database Schema

```sql
documents          -- One row per ingested file
├── id             SERIAL PRIMARY KEY
├── name           TEXT
└── ingested_at    TIMESTAMPTZ

document_tags      -- Key=value pairs, many per document
├── document_id    → documents.id
├── key            TEXT
└── value          TEXT

cache              -- Key=value pairs for caching (e.g. embedding dimensions)
├── key            TEXT PRIMARY KEY
└── value          TEXT

chunks_{dim}       -- Dynamically created per embedding dimension (e.g. chunks_768)
├── document_id    → documents.id
├── chunk_index    INTEGER
├── content        TEXT
├── embed_provider TEXT
├── embed_model    TEXT
└── embedding      vector({dim})  -- HNSW indexed
```

Tag filtering uses AND logic: all specified tags must match. The similarity search uses cosine distance via pgvector's `<=>` operator with an HNSW index for fast approximate nearest-neighbor lookup.

## Inspecting the Database

Connect to the PostgreSQL container:

```bash
docker exec -it rag-postgres psql -U raguser -d ragdb
```

### Useful queries

```sql
-- List all documents
SELECT id, name, ingested_at FROM documents ORDER BY ingested_at DESC;

-- List all tags
SELECT DISTINCT key, value FROM document_tags ORDER BY key, value;

-- Count documents per unique tag
SELECT key, value, COUNT(*) AS doc_count
FROM document_tags
GROUP BY key, value
ORDER BY doc_count DESC;

-- View cached values (e.g. embedding dimensions)
SELECT * FROM cache;

-- List all chunk tables (dynamically named chunks_{dim}, e.g. chunks_768)
SELECT tablename FROM pg_tables
WHERE schemaname = 'public' AND tablename LIKE 'chunks_%';

-- Count chunks per document (replace 768 with your embedding dimension)
SELECT d.name, COUNT(c.id) AS chunk_count
FROM documents d
JOIN chunks_768 c ON c.document_id = d.id
GROUP BY d.name
ORDER BY chunk_count DESC;

-- Show tags for a specific document
SELECT dt.key, dt.value
FROM document_tags dt
JOIN documents d ON d.id = dt.document_id
WHERE d.name = 'report.pdf';

-- Total embeddings stored (replace 768 with your embedding dimension)
SELECT COUNT(*) FROM chunks_768;

-- See which provider/model combos have been used for embeddings
SELECT DISTINCT embed_provider, embed_model FROM chunks_768;
```

Exit psql with `\q`.

## How It Works

### Ingest Pipeline

1. **Extract** — `pdf-parse` for PDFs, plain read for text files
2. **Chunk** — Mastra `MDocument` recursive character splitter tries paragraph → sentence → word → char boundaries. Default 1000 chars with 200 char overlap.
3. **Embed** — Each chunk is sent to Ollama's `nomic-embed-text` via the Vercel AI SDK, returning a 768-dim vector
4. **Store** — Document metadata, tags, and chunks (with embeddings) go into PostgreSQL

### Query Pipeline

1. **Embed** the natural language query using the same embedding model
2. **Search** pgvector using cosine similarity, optionally filtering by tags first
3. **Expand** context window by fetching surrounding chunks from the same document
4. **Synthesize** (optional) — feed retrieved chunks to the chat LLM with instructions to answer only from context

### Agent Pipeline

1. User message goes to the chat LLM along with a `search_documents` tool definition
2. The LLM decides whether/how to search (query text, tag filters, top_k)
3. Tool results are fed back; the LLM can search again or produce a final answer
4. Loop continues (up to 10 iterations) until the LLM responds without tool calls

## Swapping Components

**Different embedding model:** Change `EMBED_MODEL` in `.env`. The system auto-detects and caches the embedding dimension, creating a new `chunks_{dim}` table as needed.

**Different chat model:** Change `CHAT_MODEL`. Any Ollama model that supports tool calling works for agent mode (e.g., `mistral`, `command-r`).

**Different database:** The `db.ts` module is a thin wrapper. To swap to ChromaDB, Qdrant, or Milvus, replace that module and the `init.sql` schema.
