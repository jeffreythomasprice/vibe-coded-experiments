"""Site registry — maps site names to adapter instances."""

from __future__ import annotations

from .forums.base import ForumAdapter
from .forums.spacebattles import SpaceBattlesAdapter
from .models import SiteType
from .stories.base import StoryAdapter

_FORUM_ADAPTERS: dict[str, type[ForumAdapter]] = {
    "spacebattles": SpaceBattlesAdapter,
}

_STORY_ADAPTERS: dict[str, type[StoryAdapter]] = {}


def list_sites() -> list[tuple[str, SiteType]]:
    """Return (name, type) pairs for all registered sites."""
    sites: list[tuple[str, SiteType]] = []
    for name in sorted(_FORUM_ADAPTERS):
        sites.append((name, SiteType.FORUM))
    for name in sorted(_STORY_ADAPTERS):
        sites.append((name, SiteType.STORY))
    return sites


def get_forum_adapter(name: str) -> ForumAdapter:
    """Return a fresh adapter instance for a forum site."""
    cls = _FORUM_ADAPTERS.get(name)
    if cls is None:
        raise KeyError(f"Unknown forum site: {name!r}")
    return cls()


def get_story_adapter(name: str) -> StoryAdapter:
    """Return a fresh adapter instance for a story site."""
    cls = _STORY_ADAPTERS.get(name)
    if cls is None:
        raise KeyError(f"Unknown story site: {name!r}")
    return cls()
