from __future__ import annotations

from datetime import datetime
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .cache import Cache
    from .models import Comment, Post, Story, Subforum, Thread


class QueryAPI:
    """Read-only interface over cached scraper content.

    All methods are async and read from SQLite. No network calls happen here.
    """

    def __init__(self, cache: Cache) -> None:
        self._cache = cache

    async def subforums(self, site_name: str) -> list[Subforum]:
        return await self._cache.get_subforums(site_name)

    async def threads(
        self,
        site_name: str,
        subforum_id: str | None = None,
        updated_after: datetime | None = None,
        updated_before: datetime | None = None,
    ) -> list[Thread]:
        return await self._cache.get_threads(
            site_name, subforum_id=subforum_id,
            updated_after=updated_after, updated_before=updated_before,
        )

    async def posts(
        self,
        site_name: str,
        container_id: str,
        posted_after: datetime | None = None,
        posted_before: datetime | None = None,
        story_only: bool = False,
    ) -> list[Post]:
        return await self._cache.get_posts(
            site_name, container_id,
            posted_after=posted_after, posted_before=posted_before,
            story_only=story_only,
        )

    async def stories(
        self,
        site_name: str,
        categories: list[str] | None = None,
        tags: list[str] | None = None,
        updated_after: datetime | None = None,
    ) -> list[Story]:
        return await self._cache.get_stories(
            site_name, categories=categories, tags=tags,
            updated_after=updated_after,
        )

    async def comments(self, site_name: str, story_id: str) -> list[Comment]:
        return await self._cache.get_comments(site_name, story_id)
