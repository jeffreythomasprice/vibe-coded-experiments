# fanfic-scraper

CLI tool for scraping fanfic forums and story sites. Currently supports SpaceBattles.

## Setup

```bash
bun install
cp config.example.toml config.toml  # edit as needed
```

## Usage

```bash
# Run all configured targets from config.toml
bun start

# With options
bun start -c path/to/config.toml --verbose --no-cache

# List subforums for a site
bun start subforums spacebattles

# List threads in a subforum (by name or ID)
bun start threads spacebattles "Creative Writing"
bun start threads spacebattles "Creative Writing" --pages 3

# Fetch posts from a specific thread
bun start posts spacebattles https://forums.spacebattles.com/threads/some-story.123456/
bun start posts spacebattles https://forums.spacebattles.com/threads/some-story.123456/ --pages 5
```

## Config

See `config.example.toml` for all options. Key sections:

- `[cache]` — disk cache directory and TTL
- `[http]` — user agent, concurrency, rate limiting, retries
- `[[targets]]` — scrape targets (forum subforums, page limits, recency filter)
