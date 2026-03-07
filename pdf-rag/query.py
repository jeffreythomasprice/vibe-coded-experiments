"""Query pipeline: embed question → vector search → optional LLM synthesis."""

from __future__ import annotations

import json
from collections import defaultdict

import litellm

from config import CHAT_MODEL, CONTEXT_WINDOW, OLLAMA_BASE_URL
from db import fetch_context_chunks, get_conn, search_chunks
from embeddings import embed_single


def _expand_results_with_context(
    conn,
    results: list[dict],
    window: int,
) -> list[dict]:
    """Expand each result with surrounding chunks, merging overlapping ranges."""
    if window <= 0:
        return results

    # Group results by document_id, preserving order info
    by_doc: dict[int, list[dict]] = defaultdict(list)
    for r in results:
        by_doc[r["document_id"]].append(r)

    # For each document, compute merged index ranges and map results to them
    doc_expanded: dict[int, dict[tuple[int, int], str]] = {}
    for doc_id, doc_results in by_doc.items():
        # Build ranges and sort by start
        ranges = []
        for r in doc_results:
            lo = max(0, r["chunk_index"] - window)
            hi = r["chunk_index"] + window
            ranges.append((lo, hi))
        ranges.sort()

        # Merge overlapping ranges
        merged = [ranges[0]]
        for lo, hi in ranges[1:]:
            prev_lo, prev_hi = merged[-1]
            if lo <= prev_hi + 1:
                merged[-1] = (prev_lo, max(prev_hi, hi))
            else:
                merged.append((lo, hi))

        # Fetch each merged range once
        range_content: dict[tuple[int, int], str] = {}
        for lo, hi in merged:
            ctx_chunks = fetch_context_chunks(conn, doc_id, lo, hi)
            range_content[(lo, hi)] = "\n\n".join(c["content"] for c in ctx_chunks)

        doc_expanded[doc_id] = range_content

    # Rebuild results with expanded content
    expanded = []
    for r in results:
        doc_id = r["document_id"]
        idx = r["chunk_index"]
        # Find the merged range that contains this chunk
        for (lo, hi), content in doc_expanded[doc_id].items():
            if lo <= idx <= hi:
                expanded.append({**r, "content": content})
                break
        else:
            expanded.append(r)

    # Deduplicate: if multiple results map to the same merged range, keep only
    # the one with the highest similarity
    seen: set[tuple[int, int, int]] = set()
    deduped = []
    for r in expanded:
        doc_id = r["document_id"]
        idx = r["chunk_index"]
        for (lo, hi) in doc_expanded[doc_id]:
            if lo <= idx <= hi:
                key = (doc_id, lo, hi)
                if key not in seen:
                    seen.add(key)
                    deduped.append(r)
                break
    return deduped


def retrieve(
    query: str,
    top_k: int = 5,
    tags: dict[str, str] | None = None,
) -> list[dict]:
    """Embed the query and return the top-k matching chunks with surrounding context."""
    query_vec = embed_single(query)
    conn = get_conn()
    results = search_chunks(conn, query_vec, top_k=top_k, tags=tags)
    results = _expand_results_with_context(conn, results, CONTEXT_WINDOW)
    conn.close()
    return results


def ask(
    query: str,
    top_k: int = 5,
    tags: dict[str, str] | None = None,
) -> dict:
    """
    RAG pipeline: retrieve relevant chunks then ask the LLM to synthesise
    an answer grounded in the retrieved context.
    """
    results = retrieve(query, top_k=top_k, tags=tags)

    if not results:
        return {"answer": "No relevant documents found.", "sources": []}

    context_block = "\n\n---\n\n".join(
        f"[Source: {r['document_name']}, chunk {r['chunk_index']}, "
        f"similarity {r['similarity']:.3f}]\n{r['content']}"
        for r in results
    )

    system_prompt = (
        "You are a helpful assistant. Answer the user's question using ONLY "
        "the provided context. If the context doesn't contain enough information, "
        "say so. Cite the source document names when possible."
    )

    messages = [
        {"role": "system", "content": system_prompt},
        {
            "role": "user",
            "content": f"Context:\n{context_block}\n\nQuestion: {query}",
        },
    ]

    response = litellm.completion(
        model=CHAT_MODEL,
        messages=messages,
        api_base=OLLAMA_BASE_URL,
    )

    answer = response.choices[0].message.content

    return {
        "answer": answer,
        "sources": [
            {
                "document": r["document_name"],
                "chunk_index": r["chunk_index"],
                "similarity": r["similarity"],
                "excerpt": r["content"][:200] + "…" if len(r["content"]) > 200 else r["content"],
            }
            for r in results
        ],
    }
