from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..http import HttpClient
    from ..models import Comment, Post, Story


class StoryAdapter(ABC):
    """Abstract adapter for story-hosting sites (AO3, FFN, etc.).

    Subclass this and implement the abstract methods to add support for a
    specific story site. The scraper orchestrator handles pagination, caching,
    and rate limiting — adapters only need to parse HTML into model objects.
    """

    site_name: str
    base_url: str

    @abstractmethod
    async def get_story_list(
        self,
        http: HttpClient,
        category: str | None = None,
        tag: str | None = None,
        page: int = 1,
    ) -> tuple[list[Story], bool]:
        """Return (stories, has_next_page) from a listing/search page."""
        ...

    @abstractmethod
    async def get_story_posts(
        self, http: HttpClient, story: Story
    ) -> list[Post]:
        """Return all chapters/posts for a story."""
        ...

    async def get_comments(
        self, http: HttpClient, story: Story, page: int = 1
    ) -> tuple[list[Comment], bool]:
        """Return (comments, has_next_page). Override if site supports comments."""
        return [], False
