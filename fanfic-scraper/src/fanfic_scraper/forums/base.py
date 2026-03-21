from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..http import HttpClient
    from ..models import Post, Subforum, Thread


class ForumAdapter(ABC):
    """Abstract adapter for forum-style fanfiction sites.

    Subclass this and implement the abstract methods to add support for a
    specific forum. The scraper orchestrator handles pagination, caching,
    and rate limiting — adapters only need to parse HTML into model objects.
    """

    site_name: str
    base_url: str

    @abstractmethod
    async def get_subforums(self, http: HttpClient) -> list[Subforum]:
        """Return the subforums available on this site."""
        ...

    @abstractmethod
    async def get_thread_list(
        self, http: HttpClient, subforum: Subforum, page: int = 1
    ) -> tuple[list[Thread], bool]:
        """Return (threads, has_next_page) for a subforum listing page."""
        ...

    @abstractmethod
    async def get_posts(
        self, http: HttpClient, thread: Thread, page: int = 1
    ) -> tuple[list[Post], bool]:
        """Return (posts, has_next_page) for a thread page.

        Implementations should set is_story_post on posts where applicable.
        """
        ...

    def identify_story_posts(self, posts: list[Post]) -> list[Post]:
        """Mark which posts are 'story' posts. Override for site-specific logic.

        Default: marks the first post as a story post.
        """
        if posts:
            posts[0].is_story_post = True
        return posts
