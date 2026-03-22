# fanfic-scraper

CLI tool for scraping fanfic forums and story sites. Supports SpaceBattles forums and Archive of Our Own (AO3).

## Setup

```bash
bun install
cp config.example.toml config.toml  # edit as needed
```

## Usage

```bash
# List all available scrape targets
bun start targets

# With options
bun start -c path/to/config.toml --verbose --no-cache
```

### Config

```bash
# Print the effective configuration (defaults merged with config file)
bun start config
bun start config -c path/to/config.toml
```

### Forums (SpaceBattles)

```bash
# List subforums for a site
bun start subforums spacebattles

# List threads in a subforum (by name or ID)
bun start threads spacebattles "Creative Writing"
bun start threads spacebattles "Creative Writing" --pages 3

# Fetch posts from a specific thread
bun start posts spacebattles https://forums.spacebattles.com/threads/some-story.123456/
bun start posts spacebattles https://forums.spacebattles.com/threads/some-story.123456/ --pages 5
```

### Stories (AO3)

```bash
# List stories for a canonical tag
bun start stories ao3 "Parahumans Series - Wildbow"
bun start stories ao3 "Parahumans Series - Wildbow" --pages 3 --updated-within 30d

# List chapters for a work
bun start chapters ao3 https://archiveofourown.org/works/72610306

# Fetch chapter content (outputs HTML)
bun start content ao3 https://archiveofourown.org/works/72610306/chapters/189103486

# Fetch comments on a chapter
bun start comments ao3 https://archiveofourown.org/works/72610306/chapters/189103486
bun start comments ao3 https://archiveofourown.org/works/72610306/chapters/189103486 --pages 5
```

### Search

Search across multiple sites with fuzzy tag and keyword filtering. Sources can be defined in `config.toml` or passed as CLI arguments.

```bash
# Search configured sources (see config.toml [search.sources])
bun start search

# Search specific sources (overrides config)
bun start search ao3:"Parahumans Series - Wildbow" spacebattles:"Creative Writing"

# Filter by tags (fuzzy match)
bun start search --tags "worm,parahumans"
bun start search --tags "worm,SI" --tag-mode all   # all tags must match

# Filter by keywords in title/summary
bun start search --keywords "taylor,skitter"

# Also search first chapter/post content for keywords (slower)
bun start search --keywords "taylor" --fetch-content

# Combine filters
bun start search --tags worm --keywords taylor --favorites-only -u 30d

# Filter to specific sites from configured sources
bun start search --sites ao3
```

### Favorites & Ignore Lists

Mark threads or stories as favorites or ignored. Ignored items are hidden from `threads`/`stories` output by default.

```bash
# Manage favorites
bun start fav list
bun start fav add https://forums.spacebattles.com/threads/some-story.123456/
bun start fav remove https://forums.spacebattles.com/threads/some-story.123456/

# Manage ignore list
bun start ignore list
bun start ignore add https://forums.spacebattles.com/threads/sticky-thread.789/
bun start ignore remove https://forums.spacebattles.com/threads/sticky-thread.789/

# Filtering flags for threads/stories commands
bun start threads spacebattles "Creative Writing" --show-ignored   # include ignored items
bun start threads spacebattles "Creative Writing" --favorites-only # only show favorites
```

Output includes a prefix indicator:
```
         1140820  2026-03-21T07:11:13.000Z  Trending Stories
[IGNORE] 428998   2018-12-10T14:38:23.000Z  CrW Rules & Sticky-Signpost
[FAV]    1289084  2026-03-22T15:39:16.000Z  Ode To The Mets
```

## Config

See `config.example.toml` for all options. Key sections:

- `[cache]` — disk cache directory and TTL
- `[http]` — user agent, concurrency, rate limiting, retries
- `[lists]` — path to favorites/ignore list file (default `/tmp/fanfic-scraper/lists.toml`)
- `[search]` — default `updated_within`, `max_pages`, and `[[search.sources]]` for the search command
