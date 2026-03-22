# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

CLI tool for scraping fanfic forums and story sites. Built with Bun + TypeScript. Currently supports SpaceBattles forums; story site adapters are defined but not yet implemented.

## Commands

```bash
bun install                    # install deps
bun start                     # run with default config.toml
bun start -v                  # verbose (logs to stdout)
bun start --no-cache          # skip HTTP cache
bun start subforums spacebattles
bun start threads spacebattles "Creative Writing"
bun start posts spacebattles <thread-url>
```

No test runner or linter is configured yet.

## Tooling

Prefer `bunx` over `npx` for running package binaries (e.g. `bunx tsc --noEmit`).

## Package management

There's a global npmrc for a different project that requires auth. Always specify the default registry when installing packages:
```bash
bun i --registry=https://registry.npmjs.org <packageName>
```

## Architecture

**Adapter pattern** — two adapter interfaces define how to scrape different site types:
- `ForumAdapter` (`src/forums/base.ts`) — `getSubforums()`, `getThreadList()`, `getPosts()`. Implemented: `SpaceBattlesAdapter` (Cheerio-based XenForo scraper).
- `StoryAdapter` (`src/stories/base.ts`) — `getStoryList()`, `getChapters()`, `getChapterContent()`. No implementations yet.

Adapters are registered in `src/scraper.ts` (`FORUM_ADAPTERS` / `STORY_ADAPTERS` maps).

**HTTP layer** (`src/http.ts`) — `HttpClient` with per-host concurrency limiting, rate limiting (min delay between requests), exponential backoff retries, and optional file cache integration.

**File cache** (`src/cache.ts`) — disk-based cache keyed by SHA-256 URL hash. Stores `.meta.json` + `.body` file pairs. TTL-based expiry.

**Config** (`src/config.ts`) — TOML config loaded from `./config.toml` or `~/.config/fanfic-scraper/config.toml`. Config keys use `snake_case` in TOML but `camelCase` in TypeScript interfaces. Duration strings parsed by `parseDuration()` (e.g. `"7d"`, `"1h"`).

**CLI** (`src/index.ts`) — commander-based. Subcommands: `targets`, `subforums`, `threads`, `posts`.

**Logger** (`src/logger.ts`) — JSON lines to daily log files, optional verbose console output.
