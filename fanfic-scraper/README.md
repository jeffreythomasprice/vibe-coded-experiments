# fanfic-scraper

Async fanfiction scraper. Requires Python 3.14+, managed via [uv](https://docs.astral.sh/uv/).

```
uv run python main.py sites
uv run python main.py subforums spacebattles
uv run python main.py threads spacebattles 18 --since 7d
uv run python main.py threads spacebattles 18 --since 12h --max-pages 5
```
