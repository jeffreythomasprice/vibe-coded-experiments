"""Thin wrapper around psycopg + pgvector for chunk storage and retrieval."""

from __future__ import annotations

import psycopg
from pgvector.psycopg import register_vector

from config import DB_DSN


def get_conn() -> psycopg.Connection:
    conn = psycopg.connect(DB_DSN, autocommit=True)
    register_vector(conn)
    return conn


# ── Ingest helpers ──────────────────────────────────────────────────────

def insert_document(conn: psycopg.Connection, name: str) -> int:
    """Create a document row and return its id."""
    row = conn.execute(
        "INSERT INTO documents (name) VALUES (%s) RETURNING id", (name,)
    ).fetchone()
    return row[0]


def insert_tags(
    conn: psycopg.Connection,
    document_id: int,
    tags: dict[str, str],
) -> None:
    """Attach key=value tags to a document."""
    if not tags:
        return
    with conn.cursor() as cur:
        cur.executemany(
            "INSERT INTO document_tags (document_id, key, value) VALUES (%s, %s, %s)",
            [(document_id, k, v) for k, v in tags.items()],
        )


def insert_chunks(
    conn: psycopg.Connection,
    document_id: int,
    chunks: list[tuple[int, str, list[float]]],
) -> None:
    """Bulk-insert (chunk_index, content, embedding) rows."""
    with conn.cursor() as cur:
        cur.executemany(
            """INSERT INTO chunks (document_id, chunk_index, content, embedding)
               VALUES (%s, %s, %s, %s::vector)""",
            [(document_id, idx, text, emb) for idx, text, emb in chunks],
        )


# ── Query helpers ───────────────────────────────────────────────────────

def search_chunks(
    conn: psycopg.Connection,
    query_embedding: list[float],
    top_k: int = 5,
    tags: dict[str, str] | None = None,
) -> list[dict]:
    """
    Return the top-k most similar chunks, optionally filtered by ALL
    supplied tags (AND logic).
    """
    params: list = [query_embedding]

    tag_filter = ""
    if tags:
        # Each tag adds a required EXISTS sub-clause
        clauses = []
        for key, value in tags.items():
            clauses.append(
                "EXISTS (SELECT 1 FROM document_tags t "
                "WHERE t.document_id = c.document_id AND t.key = %s AND t.value = %s)"
            )
            params.extend([key, value])
        tag_filter = "WHERE " + " AND ".join(clauses)

    params.append(top_k)

    sql = f"""
        SELECT c.id, c.document_id, c.chunk_index, c.content,
               1 - (c.embedding <=> %s::vector) AS similarity,
               d.name AS document_name
        FROM chunks c
        JOIN documents d ON d.id = c.document_id
        {tag_filter}
        ORDER BY c.embedding <=> %s::vector
        LIMIT %s
    """
    # We need the embedding twice: once for the SELECT similarity, once for ORDER BY
    params.insert(1, query_embedding)  # second positional for ORDER BY

    # Rebuild: [emb_for_select, emb_for_order, ...tag_params..., top_k]
    # Actually simpler to just build cleanly:
    select_params: list = [query_embedding]
    order_params: list = [query_embedding]

    tag_params: list = []
    tag_clauses = []
    if tags:
        for key, value in tags.items():
            tag_clauses.append(
                "EXISTS (SELECT 1 FROM document_tags t "
                "WHERE t.document_id = c.document_id AND t.key = %s AND t.value = %s)"
            )
            tag_params.extend([key, value])

    where = ("WHERE " + " AND ".join(tag_clauses)) if tag_clauses else ""

    sql = f"""
        SELECT c.id, c.document_id, c.chunk_index, c.content,
               1 - (c.embedding <=> %s::vector) AS similarity,
               d.name AS document_name
        FROM chunks c
        JOIN documents d ON d.id = c.document_id
        {where}
        ORDER BY c.embedding <=> %s::vector
        LIMIT %s
    """
    all_params = select_params + tag_params + order_params + [top_k]

    rows = conn.execute(sql, all_params).fetchall()
    return [
        {
            "chunk_id": r[0],
            "document_id": r[1],
            "chunk_index": r[2],
            "content": r[3],
            "similarity": float(r[4]),
            "document_name": r[5],
        }
        for r in rows
    ]


def fetch_context_chunks(
    conn: psycopg.Connection,
    document_id: int,
    index_start: int,
    index_end: int,
) -> list[dict]:
    """Fetch chunks for a document within an index range, ordered by chunk_index."""
    rows = conn.execute(
        """SELECT chunk_index, content FROM chunks
           WHERE document_id = %s AND chunk_index BETWEEN %s AND %s
           ORDER BY chunk_index""",
        [document_id, index_start, index_end],
    ).fetchall()
    return [{"chunk_index": r[0], "content": r[1]} for r in rows]


# ── Listing helpers ────────────────────────────────────────────────────

def list_documents(conn: psycopg.Connection) -> list[dict]:
    """Return all documents with their tags."""
    rows = conn.execute(
        """SELECT d.id, d.name, d.ingested_at, dt.key, dt.value
           FROM documents d
           LEFT JOIN document_tags dt ON dt.document_id = d.id
           ORDER BY d.id"""
    ).fetchall()

    docs: dict[int, dict] = {}
    for r in rows:
        doc_id = r[0]
        if doc_id not in docs:
            docs[doc_id] = {
                "id": doc_id,
                "name": r[1],
                "ingested_at": r[2],
                "tags": {},
            }
        if r[3] is not None:
            docs[doc_id]["tags"][r[3]] = r[4]
    return list(docs.values())


def list_tags(conn: psycopg.Connection) -> list[dict]:
    """Return all distinct tag key=value pairs with document counts."""
    rows = conn.execute(
        """SELECT key, value, COUNT(*) AS doc_count
           FROM document_tags
           GROUP BY key, value
           ORDER BY key, value"""
    ).fetchall()
    return [{"key": r[0], "value": r[1], "doc_count": r[2]} for r in rows]


def find_documents_by_tags(
    conn: psycopg.Connection, tags: dict[str, str]
) -> list[dict]:
    """Return documents matching all given tags (AND logic)."""
    clauses = []
    params: list = []
    for key, value in tags.items():
        clauses.append(
            "EXISTS (SELECT 1 FROM document_tags t "
            "WHERE t.document_id = d.id AND t.key = %s AND t.value = %s)"
        )
        params.extend([key, value])

    where = "WHERE " + " AND ".join(clauses)
    sql = f"""
        SELECT d.id, d.name, d.ingested_at
        FROM documents d
        {where}
        ORDER BY d.name
    """
    rows = conn.execute(sql, params).fetchall()
    return [{"id": r[0], "name": r[1], "ingested_at": r[2]} for r in rows]
