from __future__ import annotations

import argparse
import asyncio
import sys
from datetime import datetime, timezone

from fanfic_scraper import ForumTarget, Scraper
from fanfic_scraper.cli import parse_duration, spinner
from fanfic_scraper.logging import setup_logging
from fanfic_scraper.sites import get_forum_adapter, list_sites


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="fanfic-scraper")
    parser.add_argument(
        "--log-file",
        default=None,
        help="Path for JSON log file (default: /tmp/fanfic-scraper-{date}.jsonl)",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable verbose logging (DEBUG level to stderr)",
    )
    sub = parser.add_subparsers(dest="command")

    # --- sites ---
    sub.add_parser("sites", help="List all registered sites")

    # --- subforums ---
    sf = sub.add_parser("subforums", help="List subforums for a forum site")
    sf.add_argument("site", help="Site name (e.g. spacebattles)")

    # --- threads ---
    th = sub.add_parser("threads", help="List threads in a subforum")
    th.add_argument("site", help="Site name (e.g. spacebattles)")
    th.add_argument("subforum", help="Subforum ID")
    th.add_argument(
        "--since",
        default=None,
        help="Only show threads updated within this duration (e.g. 12h, 7d, 2w)",
    )
    th.add_argument(
        "--max-pages",
        type=int,
        default=3,
        help="Max pages of thread listings to fetch (default: 3)",
    )

    return parser


# ---- subcommand handlers ----


async def cmd_sites() -> None:
    sites = list_sites()
    if not sites:
        print("No sites registered.")
        return
    for name, site_type in sites:
        print(f"  {name}  ({site_type.value})")


async def cmd_subforums(site: str) -> None:
    adapter = get_forum_adapter(site)

    async with Scraper(min_delay_per_host=1.5) as s:
        # Try cache first
        cached = await s.query.subforums(adapter.site_name)
        if cached:
            _print_subforums(cached)
            return

        # Fetch from network
        async with spinner(f"Fetching subforums from {site}..."):
            target = ForumTarget(adapter=adapter)
            await s.fetch_subforums(target)

        subforums = await s.query.subforums(adapter.site_name)
        _print_subforums(subforums)


def _print_subforums(subforums: list) -> None:
    if not subforums:
        print("No subforums found.")
        return
    for sf in subforums:
        print(f"  [{sf.subforum_id:>4}]  {sf.name}")


async def cmd_threads(
    site: str, subforum_id: str, since: str | None, max_pages: int
) -> None:
    adapter = get_forum_adapter(site)

    updated_after = None
    if since is not None:
        delta = parse_duration(since)
        updated_after = datetime.now(timezone.utc) - delta

    async with Scraper(min_delay_per_host=1.5) as s:
        # Fetch thread listings only (no posts)
        async with spinner(f"Fetching threads from {site} subforum {subforum_id}..."):
            target = ForumTarget(
                adapter=adapter,
                subforums=[subforum_id],
                max_thread_pages=max_pages,
            )
            await s.fetch_thread_list(target)

        threads = await s.query.threads(
            adapter.site_name,
            subforum_id=subforum_id,
            updated_after=updated_after,
        )

        if not threads:
            print("No threads found.")
            return

        for t in threads:
            age = _format_age(t.last_updated) if t.last_updated else "unknown"
            print(f"  [{t.thread_id:>8}]  {age:>6}  {t.title}")


def _format_age(dt: datetime) -> str:
    """Format a datetime as a human-readable age like '3h', '2d'."""
    now = datetime.now(timezone.utc)
    # Handle naive datetimes from the cache
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    delta = now - dt
    total_seconds = int(delta.total_seconds())
    if total_seconds < 0:
        return "now"
    if total_seconds < 60:
        return f"{total_seconds}s"
    if total_seconds < 3600:
        return f"{total_seconds // 60}m"
    if total_seconds < 86400:
        return f"{total_seconds // 3600}h"
    days = total_seconds // 86400
    if days < 7:
        return f"{days}d"
    if days < 365:
        return f"{days // 7}w"
    return f"{days // 365}y"


async def amain() -> None:
    parser = build_parser()
    args = parser.parse_args()
    setup_logging(args.log_file, verbose=args.verbose)

    match args.command:
        case "sites":
            await cmd_sites()
        case "subforums":
            await cmd_subforums(args.site)
        case "threads":
            await cmd_threads(args.site, args.subforum, args.since, args.max_pages)
        case _:
            parser.print_help()
            sys.exit(1)


def main() -> None:
    asyncio.run(amain())


if __name__ == "__main__":
    main()
