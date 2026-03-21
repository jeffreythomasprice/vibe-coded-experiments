# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Async fanfiction scraper that pulls threads/stories from forum and story-hosting sites (SpaceBattles, Sufficient Velocity, AO3) into a local SQLite cache. One concrete adapter exists (SpaceBattles/XenForo); no story-site adapters yet.

## Commands

- **Run**: `uv run python main.py`
- **Install deps**: `uv sync`
- **Requires**: Python 3.14+, managed via uv

No tests or linter configured yet.

### CLI subcommands

```bash
uv run python main.py sites                              # list registered sites
uv run python main.py subforums spacebattles              # list subforums
uv run python main.py threads spacebattles 18 --since 7d  # threads in subforum 18
```

## Architecture

The codebase follows an adapter pattern with two site types:

- **ForumAdapter** (`src/fanfic_scraper/forums/base.py`): Abstract base for forum-style sites (SpaceBattles, SV). Implement `get_subforums()`, `get_thread_list()`, `get_posts()`. Override `identify_story_posts()` for site-specific story detection.
- **StoryAdapter** (`src/fanfic_scraper/stories/base.py`): Abstract base for story sites (AO3). Implement `get_story_list()`, `get_story_posts()`. Optionally override `get_comments()`.
- **SpaceBattlesAdapter** (`src/fanfic_scraper/forums/spacebattles.py`): Concrete XenForo-based forum adapter. Parses subforums, thread listings, and posts using BeautifulSoup + lxml. Uses threadmarks to identify story posts.

Adapters are pure HTML parsers — they receive an `HttpClient` and return model dataclasses. New adapters must be registered in `sites.py` (`_FORUM_ADAPTERS` / `_STORY_ADAPTERS` dicts). The orchestration layer handles:

- **Scraper** (`scraper.py`): Main entry point. Uses `async with Scraper() as s:` pattern. Drives pagination, concurrency (via `TaskGroup`), and delegates to adapters.
- **HttpClient** (`http.py`): Per-host throttling (semaphore + min delay), retry with backoff, 429 handling, transparent HTTP response caching via SQLite.
- **Cache** (`cache.py`): SQLite persistence for both HTTP responses and parsed domain objects. Also serves as the read-side data store.
- **QueryAPI** (`query.py`): Read-only async interface over cached data, exposed as `scraper.query`.

Scraping is configured via **ForumTarget** / **StoryTarget** dataclasses (`targets.py`) which pair an adapter with filters (subforum IDs, categories, tags, date range, page limits).

All domain models are plain dataclasses in `models.py`: `Subforum`, `Thread`, `Post`, `Story`, `Comment`.

All I/O is async. Concurrency uses `asyncio.TaskGroup`. Structured JSON logging goes to `/tmp/fanfic-scraper-{date}.jsonl` by default (configured in `logging.py`).
