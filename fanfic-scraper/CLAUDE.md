# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

CLI tool for scraping fanfic forums and story sites. Built with Bun + TypeScript. Supports SpaceBattles and SufficientVelocity forums (both XenForo-based) and Archive of Our Own (AO3) story listings.

## Commands

```bash
bun install                    # install deps
bun start                     # run with default config.toml
bun start -v                  # verbose (logs to stdout)
bun start --no-cache          # skip HTTP cache
bun start targets             # list all available scrape targets
bun start config              # print effective config as TOML
```

### Forum commands (SpaceBattles, SufficientVelocity)

```bash
bun start subforums spacebattles
bun start threads spacebattles "Creative Writing"
bun start posts spacebattles <thread-url>

bun start subforums sufficientvelocity
bun start threads sufficientvelocity "Quests"
bun start posts sufficientvelocity <thread-url>
```

### Story commands (AO3)

```bash
# List stories for a canonical tag (paginated, respects updatedWithin cutoff)
bun start stories ao3 "Parahumans Series - Wildbow"
bun start stories ao3 "Parahumans Series - Wildbow" -p 3 -u 30d

# List chapters for a work
bun start chapters ao3 https://archiveofourown.org/works/72610306

# Fetch chapter content (HTML)
bun start content ao3 https://archiveofourown.org/works/72610306/chapters/189103486

# Fetch comments on a chapter
bun start comments ao3 https://archiveofourown.org/works/72610306/chapters/189103486
bun start comments ao3 <chapter-url> -p 5   # up to 5 pages of comments
```

### Search command

```bash
# Search across configured sources (defined in config.toml [search.sources])
bun start search

# Search specific sources (overrides config)
bun start search ao3:"Parahumans Series - Wildbow" spacebattles:"Creative Writing"

# Filter by tags (fuzzy match, any mode)
bun start search --tags "worm,parahumans"

# Filter by tags (all must match)
bun start search --tags "worm,SI" --tag-mode all

# Filter by keywords in title/summary
bun start search --keywords "taylor,skitter"

# Also search first chapter/post content for keywords (slower)
bun start search --keywords "taylor" --fetch-content

# Combine filters
bun start search --tags worm --keywords taylor --favorites-only -u 30d

# Filter to specific sites from configured sources
bun start search --sites ao3
```

### Favorites & ignore list commands

```bash
bun start fav list                     # show all favorites
bun start fav add <url>                # add URL to favorites
bun start fav remove <url>             # remove URL from favorites

bun start ignore list                  # show all ignored
bun start ignore add <url>             # add URL to ignore list
bun start ignore remove <url>          # remove URL from ignore list
```

The `threads` and `stories` commands hide ignored items by default. Use `--show-ignored` to include them, or `--favorites-only` to only show favorites.

No test runner is configured yet.

## Linting and formatting

```bash
bun run lint                  # eslint
bun run lint:fix              # eslint --fix
bun run format                # prettier --write
bun run format:check          # prettier --check
bunx tsc --noEmit             # type-check
```

ESLint uses flat config (`eslint.config.js`) with typescript-eslint + prettier integration. Unused vars prefixed with `_` are allowed.

## Tooling

Prefer `bunx` over `npx` for running package binaries (e.g. `bunx tsc --noEmit`).

## Package management

There's a global npmrc for a different project that requires auth. Always specify the default registry when installing packages:
```bash
bun i --registry=https://registry.npmjs.org <packageName>
```

## Architecture

**Adapter pattern** — two adapter interfaces define how to scrape different site types:
- `ForumAdapter` (`src/forums/base.ts`) — `getSubforums()`, `getThreadList()`, `getPosts()`. Base implementation: `XenForoAdapter` (`src/forums/xenforo.ts`). Subclasses: `SpaceBattlesAdapter`, `SufficientVelocityAdapter`.
- `StoryAdapter` (`src/stories/base.ts`) — `getStoryList()`, `getChapters()`, `getChapterContent()`, `getComments()`. Implemented: `AO3Adapter` (Cheerio-based AO3 scraper).

Adapters are registered in `src/scraper.ts` (`FORUM_ADAPTERS` / `STORY_ADAPTERS` maps).

**HTTP layer** (`src/http.ts`) — `HttpClient` with per-host concurrency limiting, rate limiting (min delay between requests), exponential backoff retries, and optional file cache integration.

**File cache** (`src/cache.ts`) — disk-based cache keyed by SHA-256 URL hash. Stores `.meta.json` + `.body` file pairs. TTL-based expiry.

**Config** (`src/config.ts`) — TOML config loaded from `./config.toml` or `~/.config/fanfic-scraper/config.toml`. Config keys use `snake_case` in TOML but `camelCase` in TypeScript interfaces. Duration strings parsed by `parseDuration()` (e.g. `"7d"`, `"1h"`).

**CLI** (`src/index.ts`) — commander-based. Subcommands: `targets`, `subforums`, `threads`, `posts`, `stories`, `chapters`, `content`, `comments`, `search`, `fav`, `ignore`.

**Search** (`src/search.ts`) — cross-site search with fuzzy tag/keyword filtering (Fuse.js). Aggregates results from multiple configured sources (forum subforums + story tags), applies filter pipeline: list status → tags → keywords (with optional content fetch). Sources configured in `[search]` config section or passed as CLI args.

**Lists** (`src/lists.ts`) — favorites/ignore list persistence in TOML format. Path configurable via `[lists] file` in config (default `/tmp/fanfic-scraper/lists.toml`).

**Logger** (`src/logger.ts`) — JSON lines to daily log files, optional verbose console output.
