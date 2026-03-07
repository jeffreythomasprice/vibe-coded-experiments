# Local RAG System

A fully local RAG (Retrieval-Augmented Generation) system using **Ollama** for LLM inference and embeddings, **PostgreSQL + pgvector** for vector storage, and **LiteLLM** as the unified SDK layer.

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
| Language | Python | Best RAG ecosystem, first-class LLM library support |
| LLM SDK | LiteLLM | Unified OpenAI-compatible API, routes to Ollama natively, supports tool calling |
| Database | PostgreSQL + pgvector | Mature, single DB for metadata + vectors, great Docker support, HNSW indexing |
| Embedding model | `nomic-embed-text` | 768-dim, 8192-token context, solid quality, runs well on CPU |
| Chat model | `glm-4.7-flash` | Good tool-calling support, strong general reasoning |
| PDF extraction | PyMuPDF | Fast, reliable, pure Python |

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

### 3. Install Python dependencies

```bash
uv sync
```

### 4. Configure (optional)

Copy the template and edit as needed:

```bash
cp .env.template .env
```

The `.env` file contains all settings with sensible defaults:

```
OLLAMA_BASE_URL=http://localhost:11434
EMBED_MODEL=ollama/nomic-embed-text
CHAT_MODEL=ollama/glm-4.7-flash
DB_DSN=postgresql://raguser:ragpass@localhost:5432/ragdb
CHUNK_SIZE=1000
CHUNK_OVERLAP=200
```

The `.env` file is gitignored so your local settings won't be committed. Environment variables still take precedence if set.

If you're running Ollama in Docker (as in the compose file), use port `11434` (the host-mapped port). If you already have Ollama running locally on `11434`, either stop it or change the compose port mapping.

## Usage

### Ingest a file

```bash
# Plain text
uv run cli.py ingest notes.txt -t project=alpha -t author=jeff

# PDF
uv run cli.py ingest report.pdf -t project=beta -t type=quarterly

# Entire directory
uv run cli.py ingest-dir ./documents/ -t project=alpha --ext .pdf --ext .txt
```

The `filename` tag is always added automatically. Additional tags are arbitrary key=value pairs.

### Query (raw vector search)

```bash
# Semantic search — returns matching chunks
uv run cli.py query "What were the Q3 revenue figures?" --raw

# Filter by tags
uv run cli.py query "deployment architecture" --raw -t project=alpha

# Adjust result count
uv run cli.py query "error handling patterns" --raw -k 10
```

### Query (with LLM synthesis)

```bash
# The LLM reads retrieved chunks and produces a grounded answer
uv run cli.py query "Summarize the key findings from the quarterly report"

# With tag filters
uv run cli.py query "What risks were identified?" -t type=quarterly
```

### Agent mode

The agent autonomously decides when and how to search. It can make multiple searches with different queries and filters before answering.

```bash
uv run cli.py agent "Compare the deployment approaches described in the alpha and beta project docs"
```

### Browse & filter

```bash
# List all ingested documents with their tags
uv run cli.py documents

# List all tags and how many documents use each one
uv run cli.py tags

# Find documents matching specific tags (AND logic)
uv run cli.py find -t project=alpha
uv run cli.py find -t project=alpha -t type=quarterly
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

chunks             -- Text chunks with embeddings
├── document_id    → documents.id
├── chunk_index    INTEGER
├── content        TEXT
└── embedding      vector(768)   -- HNSW indexed
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

-- Count chunks per document
SELECT d.name, COUNT(c.id) AS chunk_count
FROM documents d
JOIN chunks c ON c.document_id = d.id
GROUP BY d.name
ORDER BY chunk_count DESC;

-- Show tags for a specific document
SELECT dt.key, dt.value
FROM document_tags dt
JOIN documents d ON d.id = dt.document_id
WHERE d.name = 'report.pdf';

-- Total embeddings stored
SELECT COUNT(*) FROM chunks;
```

Exit psql with `\q`.

## How It Works

### Ingest Pipeline

1. **Extract** — PyMuPDF for PDFs, plain read for text files
2. **Chunk** — Recursive character splitter tries paragraph → sentence → word → char boundaries. Default 1000 chars with 200 char overlap.
3. **Embed** — Each chunk is sent to Ollama's `nomic-embed-text` via LiteLLM, returning a 768-dim vector
4. **Store** — Document metadata, tags, and chunks (with embeddings) go into PostgreSQL

### Query Pipeline

1. **Embed** the natural language query using the same embedding model
2. **Search** pgvector using cosine similarity, optionally filtering by tags first
3. **Synthesize** (optional) — feed retrieved chunks to the chat LLM with instructions to answer only from context

### Agent Pipeline

1. User message goes to the chat LLM along with a `search_documents` tool definition
2. The LLM decides whether/how to search (query text, tag filters, top_k)
3. Tool results are fed back; the LLM can search again or produce a final answer
4. Loop continues (up to 10 iterations) until the LLM responds without tool calls

## Swapping Components

**Different embedding model:** Change `EMBED_MODEL` and update the vector dimension in `init.sql`. For example, `mxbai-embed-large` produces 1024-dim vectors.

**Different chat model:** Change `CHAT_MODEL`. Any Ollama model that supports tool calling works for agent mode (e.g., `mistral`, `command-r`).

**Different database:** The `src/db.py` module is a thin ~100-line wrapper. To swap to ChromaDB, Qdrant, or Milvus, replace that module and the `init.sql` schema.

**Different LLM provider:** LiteLLM supports 100+ providers. Change the model string prefix (e.g., `openai/gpt-4o`, `anthropic/claude-3.5-sonnet`) and set the relevant API key.