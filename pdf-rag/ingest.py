"""Ingest pipeline: file → text → chunks → embeddings → database."""

from __future__ import annotations

import os
from pathlib import Path

from rich.console import Console
from rich.progress import track

from db import get_conn, insert_document, insert_tags, insert_chunks
from text import extract_text, chunk_text
from embeddings import embed_texts

console = Console()

# Max texts per embedding API call (Ollama handles one at a time anyway,
# but batching keeps the interface clean for other providers)
EMBED_BATCH_SIZE = 32


def ingest_file(filepath: str, tags: dict[str, str] | None = None) -> dict:
    """
    Full ingest pipeline for a single file.

    Returns a summary dict with document_id, chunk_count, etc.
    """
    filepath = os.path.abspath(filepath)
    filename = Path(filepath).name
    tags = tags or {}

    # Always include the filename as a tag
    tags.setdefault("filename", filename)

    console.print(f"[bold]Ingesting:[/bold] {filepath}")

    # 1. Extract text
    console.print("  Extracting text…")
    text = extract_text(filepath)
    if not text.strip():
        console.print("[red]  No text extracted — skipping.[/red]")
        return {"error": "no text extracted"}

    console.print(f"  Extracted {len(text):,} characters")

    # 2. Chunk
    chunks = chunk_text(text)
    console.print(f"  Split into {len(chunks)} chunks")

    # 3. Embed (in batches)
    console.print("  Generating embeddings…")
    all_embeddings: list[list[float]] = []
    for i in range(0, len(chunks), EMBED_BATCH_SIZE):
        batch = chunks[i : i + EMBED_BATCH_SIZE]
        all_embeddings.extend(embed_texts(batch))

    # 4. Store
    console.print("  Storing in database…")
    conn = get_conn()
    doc_id = insert_document(conn, filename)
    insert_tags(conn, doc_id, tags)
    insert_chunks(
        conn,
        doc_id,
        [(i, text, emb) for i, (text, emb) in enumerate(zip(chunks, all_embeddings))],
    )
    conn.close()

    summary = {
        "document_id": doc_id,
        "filename": filename,
        "characters": len(text),
        "chunks": len(chunks),
        "tags": tags,
    }
    console.print(f"  [green]Done![/green] document_id={doc_id}, {len(chunks)} chunks stored")
    return summary
