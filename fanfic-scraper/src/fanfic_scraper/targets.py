from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime

from .forums.base import ForumAdapter
from .stories.base import StoryAdapter


@dataclass
class ForumTarget:
    """Specifies what to scrape from a forum site."""

    adapter: ForumAdapter
    subforums: list[str] | None = None  # subforum IDs; None = all
    updated_after: datetime | None = None
    max_thread_pages: int = 5


@dataclass
class StoryTarget:
    """Specifies what to scrape from a story site."""

    adapter: StoryAdapter
    categories: list[str] | None = None
    tags: list[str] | None = None
    updated_after: datetime | None = None
    fetch_comments: bool = False
    max_listing_pages: int = 10
