#!/usr/bin/env python3
"""CLI for the local RAG system."""

from __future__ import annotations

import json
import click
from rich.console import Console
from rich.table import Table

console = Console()


def _parse_tags(tag_strings: tuple[str, ...]) -> dict[str, str]:
    """Parse 'key=value' strings into a dict."""
    tags = {}
    for t in tag_strings:
        if "=" not in t:
            raise click.BadParameter(f"Tag must be key=value, got: {t}")
        k, v = t.split("=", 1)
        tags[k.strip()] = v.strip()
    return tags


@click.group()
def cli():
    """Local RAG system powered by Ollama + pgvector."""
    pass


# ── Ingest ──────────────────────────────────────────────────────────────

@cli.command()
@click.argument("filepath", type=click.Path(exists=True))
@click.option("-t", "--tag", "tags", multiple=True, help="Tag as key=value (repeatable)")
def ingest(filepath: str, tags: tuple[str, ...]):
    """Ingest a file (txt or pdf) into the vector store."""
    from ingest import ingest_file

    tag_dict = _parse_tags(tags)
    result = ingest_file(filepath, tags=tag_dict)
    console.print_json(json.dumps(result, default=str))


@cli.command()
@click.argument("directory", type=click.Path(exists=True, file_okay=False))
@click.option("-t", "--tag", "tags", multiple=True, help="Tag as key=value (repeatable)")
@click.option("--ext", multiple=True, default=(".txt", ".pdf"), help="File extensions to include")
def ingest_dir(directory: str, tags: tuple[str, ...], ext: tuple[str, ...]):
    """Ingest all matching files in a directory."""
    from pathlib import Path
    from ingest import ingest_file

    tag_dict = _parse_tags(tags)
    files = [f for f in Path(directory).rglob("*") if f.suffix.lower() in ext]

    if not files:
        console.print(f"[yellow]No files matching {ext} found in {directory}[/yellow]")
        return

    console.print(f"Found {len(files)} file(s) to ingest")
    for f in files:
        per_file_tags = {**tag_dict, "filename": f.name}
        ingest_file(str(f), tags=per_file_tags)


# ── Query ───────────────────────────────────────────────────────────────

@cli.command()
@click.argument("query")
@click.option("-t", "--tag", "tags", multiple=True, help="Filter by tag key=value")
@click.option("-k", "--top-k", default=5, help="Number of results")
@click.option("--raw", is_flag=True, help="Return raw chunks without LLM synthesis")
def query(query: str, tags: tuple[str, ...], top_k: int, raw: bool):
    """Query the vector store with natural language."""
    tag_dict = _parse_tags(tags) if tags else None

    if raw:
        from query import retrieve
        results = retrieve(query, top_k=top_k, tags=tag_dict)
        _print_results_table(results)
    else:
        from query import ask
        result = ask(query, top_k=top_k, tags=tag_dict)
        console.print(f"\n[bold]Answer:[/bold]\n{result['answer']}\n")
        if result["sources"]:
            console.print("[dim]Sources:[/dim]")
            for s in result["sources"]:
                console.print(
                    f"  • {s['document']} (chunk {s['chunk_index']}, "
                    f"similarity {s['similarity']:.3f})"
                )


@cli.command()
@click.argument("message")
def agent(message: str):
    """Ask the agentic assistant (LLM decides when/how to search)."""
    from agent import agent_chat

    answer = agent_chat(message)
    console.print(f"\n[bold]Agent:[/bold]\n{answer}")


# ── Browse & filter ────────────────────────────────────────────────────

@cli.command()
def documents():
    """List all ingested documents with their tags."""
    from db import get_conn, list_documents

    with get_conn() as conn:
        docs = list_documents(conn)

    if not docs:
        console.print("[yellow]No documents found.[/yellow]")
        return

    table = Table(title="Documents")
    table.add_column("ID", style="dim")
    table.add_column("Name")
    table.add_column("Ingested At")
    table.add_column("Tags")

    for d in docs:
        tag_str = ", ".join(f"{k}={v}" for k, v in d["tags"].items())
        table.add_row(
            str(d["id"]),
            d["name"],
            str(d["ingested_at"]),
            tag_str,
        )
    console.print(table)


@cli.command()
def tags():
    """List all tags and how many documents use each one."""
    from db import get_conn, list_tags

    with get_conn() as conn:
        tag_list = list_tags(conn)

    if not tag_list:
        console.print("[yellow]No tags found.[/yellow]")
        return

    table = Table(title="Tags")
    table.add_column("Key")
    table.add_column("Value")
    table.add_column("Documents", justify="right")

    for t in tag_list:
        table.add_row(t["key"], t["value"], str(t["doc_count"]))
    console.print(table)


@cli.command()
@click.option("-t", "--tag", "tags", multiple=True, required=True, help="Filter by tag key=value (repeatable)")
def find(tags: tuple[str, ...]):
    """Find documents matching all given tags (AND logic)."""
    from db import get_conn, find_documents_by_tags

    tag_dict = _parse_tags(tags)

    with get_conn() as conn:
        docs = find_documents_by_tags(conn, tag_dict)

    if not docs:
        console.print("[yellow]No documents match the given tags.[/yellow]")
        return

    table = Table(title="Matching Documents")
    table.add_column("ID", style="dim")
    table.add_column("Name")
    table.add_column("Ingested At")

    for d in docs:
        table.add_row(str(d["id"]), d["name"], str(d["ingested_at"]))
    console.print(table)


# ── Utilities ───────────────────────────────────────────────────────────

def _print_results_table(results: list[dict]):
    table = Table(title="Search Results")
    table.add_column("#", style="dim")
    table.add_column("Document")
    table.add_column("Chunk")
    table.add_column("Similarity", justify="right")
    table.add_column("Content", max_width=80)

    for i, r in enumerate(results, 1):
        excerpt = r["content"][:150].replace("\n", " ")
        table.add_row(
            str(i),
            r["document_name"],
            str(r["chunk_index"]),
            f"{r['similarity']:.4f}",
            excerpt + "…",
        )
    console.print(table)


if __name__ == "__main__":
    cli()
