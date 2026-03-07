"""Extract text from files and split into chunks."""

from __future__ import annotations

import os
from pathlib import Path

import fitz  # pymupdf

from config import CHUNK_SIZE, CHUNK_OVERLAP


def extract_text(filepath: str) -> str:
    """Return plain text from a .txt or .pdf file."""
    ext = Path(filepath).suffix.lower()
    if ext == ".pdf":
        return _extract_pdf(filepath)
    # Default: read as plain text
    return Path(filepath).read_text(encoding="utf-8", errors="replace")


def _extract_pdf(filepath: str) -> str:
    doc = fitz.open(filepath)
    pages = [page.get_text() for page in doc]
    doc.close()
    return "\n\n".join(pages)


def chunk_text(
    text: str,
    chunk_size: int = CHUNK_SIZE,
    overlap: int = CHUNK_OVERLAP,
) -> list[str]:
    """
    Recursive character splitter.

    Tries to split on paragraph breaks first, then sentences, then words,
    falling back to hard character splits.
    """
    separators = ["\n\n", "\n", ". ", " ", ""]
    return _split_recursive(text, separators, chunk_size, overlap)


def _split_recursive(
    text: str,
    separators: list[str],
    chunk_size: int,
    overlap: int,
) -> list[str]:
    if len(text) <= chunk_size:
        stripped = text.strip()
        return [stripped] if stripped else []

    sep = separators[0]
    remaining_seps = separators[1:] if len(separators) > 1 else [""]

    if sep == "":
        # Hard character split
        chunks = []
        start = 0
        while start < len(text):
            end = start + chunk_size
            chunks.append(text[start:end].strip())
            start = end - overlap
        return [c for c in chunks if c]

    parts = text.split(sep)
    chunks: list[str] = []
    current = ""

    for part in parts:
        candidate = (current + sep + part) if current else part
        if len(candidate) <= chunk_size:
            current = candidate
        else:
            if current:
                chunks.append(current.strip())
            # If this single part exceeds chunk_size, split it further
            if len(part) > chunk_size:
                chunks.extend(_split_recursive(
                    part, remaining_seps, chunk_size, overlap))
                current = ""
            else:
                current = part

    if current.strip():
        chunks.append(current.strip())

    # Apply overlap: prepend tail of previous chunk to next chunk
    if overlap > 0 and len(chunks) > 1:
        overlapped = [chunks[0]]
        for i in range(1, len(chunks)):
            prev_tail = chunks[i - 1][-overlap:]
            merged = prev_tail + " " + chunks[i]
            # Only add overlap if it doesn't make the chunk too huge
            if len(merged) <= chunk_size * 1.5:
                overlapped.append(merged)
            else:
                overlapped.append(chunks[i])
        chunks = overlapped

    return [c for c in chunks if c]
