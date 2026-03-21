from __future__ import annotations

import asyncio
import logging
from typing import TYPE_CHECKING

from .cache import Cache
from .http import HttpClient
from .query import QueryAPI
from .targets import ForumTarget, StoryTarget

if TYPE_CHECKING:
    from .models import Thread

log = logging.getLogger(__name__)

# Safety limit to prevent infinite pagination from buggy adapters
_MAX_PAGES_HARD_LIMIT = 200


class Scraper:
    """Main entry point. Orchestrates fetching and caching."""

    def __init__(
        self,
        db_path: str = "fanfic_cache.db",
        max_concurrent_per_host: int = 2,
        min_delay_per_host: float = 1.0,
    ) -> None:
        self._db_path = db_path
        self._max_concurrent = max_concurrent_per_host
        self._min_delay = min_delay_per_host
        self._cache: Cache | None = None
        self._http: HttpClient | None = None
        self._query: QueryAPI | None = None

    async def __aenter__(self) -> Scraper:
        self._cache = Cache(self._db_path)
        await self._cache.open()
        self._http = HttpClient(
            max_concurrent_per_host=self._max_concurrent,
            min_delay_per_host=self._min_delay,
            cache=self._cache,
        )
        await self._http.__aenter__()
        self._query = QueryAPI(self._cache)
        return self

    async def __aexit__(self, *exc: object) -> None:
        if self._http:
            await self._http.__aexit__(*exc)
        if self._cache:
            await self._cache.close()

    @property
    def query(self) -> QueryAPI:
        assert self._query is not None, "Scraper not opened — use 'async with Scraper() as s:'"
        return self._query

    async def run(self, targets: list[ForumTarget | StoryTarget]) -> None:
        """Execute all scraping targets."""
        for target in targets:
            if isinstance(target, ForumTarget):
                await self._run_forum_target(target)
            elif isinstance(target, StoryTarget):
                await self._run_story_target(target)
            else:
                raise TypeError(f"Unknown target type: {type(target)}")

    async def fetch_subforums(self, target: ForumTarget) -> None:
        """Fetch and cache subforums only (no threads or posts)."""
        assert self._http and self._cache
        adapter = target.adapter
        subforums = await adapter.get_subforums(self._http)
        if target.subforums is not None:
            subforums = [sf for sf in subforums if sf.subforum_id in target.subforums]
        for sf in subforums:
            await self._cache.upsert_subforum(sf)

    async def fetch_thread_list(self, target: ForumTarget) -> None:
        """Fetch and cache subforums + thread listings (no posts)."""
        assert self._http and self._cache
        adapter = target.adapter
        log.info("fetch_threads_start", extra={"site": adapter.site_name})

        subforums = await adapter.get_subforums(self._http)
        if target.subforums is not None:
            subforums = [sf for sf in subforums if sf.subforum_id in target.subforums]
        for sf in subforums:
            await self._cache.upsert_subforum(sf)

        all_threads: list[Thread] = []
        async with asyncio.TaskGroup() as tg:
            for sf in subforums:
                tg.create_task(self._fetch_thread_list(target, sf, all_threads))

        log.info("fetch_threads_done", extra={"site": adapter.site_name, "count": len(all_threads)})

    async def _run_forum_target(self, target: ForumTarget) -> None:
        assert self._http and self._cache
        adapter = target.adapter
        log.info("scrape_forum_start", extra={"site": adapter.site_name})

        # Fetch subforums
        subforums = await adapter.get_subforums(self._http)
        if target.subforums is not None:
            subforums = [sf for sf in subforums if sf.subforum_id in target.subforums]

        for sf in subforums:
            log.info("subforum_found", extra={"subforum_id": sf.subforum_id, "subforum_name": sf.name})
            await self._cache.upsert_subforum(sf)

        # Gather threads from all subforums concurrently
        all_threads: list[Thread] = []
        async with asyncio.TaskGroup() as tg:
            for sf in subforums:
                tg.create_task(self._fetch_thread_list(target, sf, all_threads))

        # Filter by recency
        if target.updated_after is not None:
            all_threads = [
                t for t in all_threads
                if t.last_updated and t.last_updated >= target.updated_after
            ]

        log.info("threads_filtered", extra={"site": adapter.site_name, "count": len(all_threads)})

        # Fetch posts for each thread concurrently
        async with asyncio.TaskGroup() as tg:
            for thread in all_threads:
                tg.create_task(self._fetch_thread_posts(target, thread))

    async def _fetch_thread_list(
        self, target: ForumTarget, subforum, out: list[Thread]
    ) -> None:
        assert self._http and self._cache
        adapter = target.adapter
        max_pages = min(target.max_thread_pages, _MAX_PAGES_HARD_LIMIT)

        for page in range(1, max_pages + 1):
            threads, has_next = await adapter.get_thread_list(self._http, subforum, page)
            log.info(
                "thread_list_page",
                extra={
                    "subforum": subforum.subforum_id,
                    "page": page,
                    "count": len(threads),
                    "has_next": has_next,
                },
            )
            for t in threads:
                await self._cache.upsert_thread(t)
                out.append(t)
            if not has_next:
                break

    async def _fetch_thread_posts(self, target: ForumTarget, thread: Thread) -> None:
        assert self._http and self._cache
        adapter = target.adapter
        all_posts = []

        for page in range(1, _MAX_PAGES_HARD_LIMIT + 1):
            posts, has_next = await adapter.get_posts(self._http, thread, page)
            all_posts.extend(posts)
            if not has_next:
                break

        all_posts = adapter.identify_story_posts(all_posts)
        await self._cache.upsert_many_posts(all_posts)
        story_count = sum(1 for p in all_posts if p.is_story_post)
        log.info(
            "thread_posts_done",
            extra={"thread_id": thread.thread_id, "post_count": len(all_posts), "story_posts": story_count},
        )

    async def _run_story_target(self, target: StoryTarget) -> None:
        assert self._http and self._cache
        adapter = target.adapter
        log.info("scrape_story_start", extra={"site": adapter.site_name})

        # Build list of (category, tag) combinations to search
        search_params: list[tuple[str | None, str | None]] = []
        if target.categories and target.tags:
            for cat in target.categories:
                for tag in target.tags:
                    search_params.append((cat, tag))
        elif target.categories:
            search_params = [(cat, None) for cat in target.categories]
        elif target.tags:
            search_params = [(None, tag) for tag in target.tags]
        else:
            search_params = [(None, None)]

        all_stories = []
        seen_ids: set[str] = set()

        for category, tag in search_params:
            max_pages = min(target.max_listing_pages, _MAX_PAGES_HARD_LIMIT)
            for page in range(1, max_pages + 1):
                stories, has_next = await adapter.get_story_list(
                    self._http, category=category, tag=tag, page=page,
                )
                for story in stories:
                    if story.story_id not in seen_ids:
                        seen_ids.add(story.story_id)
                        all_stories.append(story)
                        await self._cache.upsert_story(story)
                if not has_next:
                    break

        # Filter by recency
        if target.updated_after is not None:
            all_stories = [
                s for s in all_stories
                if s.last_updated and s.last_updated >= target.updated_after
            ]

        log.info("stories_filtered", extra={"site": adapter.site_name, "count": len(all_stories)})

        # Fetch posts and comments concurrently
        async with asyncio.TaskGroup() as tg:
            for story in all_stories:
                tg.create_task(self._fetch_story_content(target, story))

    async def _fetch_story_content(self, target: StoryTarget, story) -> None:
        assert self._http and self._cache
        adapter = target.adapter

        # Fetch chapters/posts
        posts = await adapter.get_story_posts(self._http, story)
        await self._cache.upsert_many_posts(posts)
        log.info("story_posts_done", extra={"story_id": story.story_id, "post_count": len(posts)})

        # Optionally fetch comments
        if target.fetch_comments:
            all_comments = []
            for page in range(1, _MAX_PAGES_HARD_LIMIT + 1):
                comments, has_next = await adapter.get_comments(
                    self._http, story, page=page,
                )
                all_comments.extend(comments)
                if not has_next:
                    break
            if all_comments:
                await self._cache.upsert_many_comments(all_comments)
                log.info("story_comments_done", extra={"story_id": story.story_id, "comment_count": len(all_comments)})
