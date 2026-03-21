from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum


class SiteType(Enum):
    FORUM = "forum"
    STORY = "story"


@dataclass
class Subforum:
    site_name: str
    subforum_id: str
    name: str
    url: str


@dataclass
class Thread:
    site_name: str
    subforum_id: str
    thread_id: str
    title: str
    url: str
    last_updated: datetime | None = None


@dataclass
class Post:
    site_name: str
    post_id: str
    container_id: str  # thread_id or story_id
    author: str
    content: str  # raw HTML
    posted_at: datetime | None = None
    is_story_post: bool = False
    ordinal: int = 0


@dataclass
class Story:
    site_name: str
    story_id: str
    title: str
    author: str
    url: str
    summary: str = ""
    last_updated: datetime | None = None
    categories: list[str] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)


@dataclass
class Comment:
    site_name: str
    comment_id: str
    story_id: str
    author: str
    content: str
    posted_at: datetime | None = None
