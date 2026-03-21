"""SpaceBattles forum adapter (XenForo-based)."""

from __future__ import annotations

import logging
import re
from datetime import datetime, timezone
from typing import TYPE_CHECKING
from urllib.parse import urljoin

from bs4 import BeautifulSoup, Tag

from ..models import Post, Subforum, Thread
from .base import ForumAdapter

if TYPE_CHECKING:
    from ..http import HttpClient

log = logging.getLogger(__name__)

# Regex to extract the numeric ID from XenForo class names like "node--id18"
_NODE_ID_RE = re.compile(r"node--id(\d+)")
# Extract thread ID from class like "js-threadListItem-12345"
_THREAD_LIST_ITEM_RE = re.compile(r"js-threadListItem-(\d+)")
# Extract thread ID from URL like "/threads/some-title.12345/"
_THREAD_URL_ID_RE = re.compile(r"/threads/[^/]*\.(\d+)")
_POST_ID_RE = re.compile(r"post-(\d+)")


class SpaceBattlesAdapter(ForumAdapter):
    site_name = "spacebattles"
    base_url = "https://forums.spacebattles.com"

    def __init__(self) -> None:
        # thread_id -> set of post IDs that are threadmarked
        self._threadmarks: dict[str, set[str]] = {}

    async def get_subforums(self, http: HttpClient) -> list[Subforum]:
        resp = await http.get(f"{self.base_url}/forums/")
        soup = BeautifulSoup(resp.content, "lxml")

        results: list[Subforum] = []
        for node in soup.select("div.node--forum"):
            # Extract numeric ID from class like "node--id18"
            node_id = _extract_node_id(node)
            if node_id is None:
                continue

            title_link = node.select_one("h3.node-title a")
            if title_link is None:
                continue

            name = title_link.get_text(strip=True)
            href = title_link.get("href", "")
            url = urljoin(self.base_url, str(href))

            results.append(
                Subforum(
                    site_name=self.site_name,
                    subforum_id=node_id,
                    name=name,
                    url=url,
                )
            )

        log.info("parse_subforums", extra={"count": len(results)})
        return results

    async def get_thread_list(
        self, http: HttpClient, subforum: Subforum, page: int = 1
    ) -> tuple[list[Thread], bool]:
        url = subforum.url
        if page > 1:
            url = url.rstrip("/") + f"/page-{page}"

        resp = await http.get(url, max_age=300)  # 5 min TTL for thread listings
        soup = BeautifulSoup(resp.content, "lxml")

        threads: list[Thread] = []
        for item in soup.select("div.structItem--thread"):
            # Extract thread ID from class like "js-threadListItem-12345"
            thread_id = _extract_thread_id(item)
            if thread_id is None:
                continue

            # Title link in .structItem-title
            title_el = item.select_one(".structItem-title")
            if title_el is None:
                continue
            title_links = title_el.select("a")
            # Last <a> is the thread link; earlier ones may be prefix labels
            thread_link = title_links[-1] if title_links else None
            if thread_link is None:
                continue

            title = thread_link.get_text(strip=True)
            href = thread_link.get("href", "")
            thread_url = urljoin(self.base_url, str(href))

            # Last updated: from the "latest" cell (last reply time)
            latest_cell = item.select_one(".structItem-cell--latest")
            last_updated = _parse_time(latest_cell) if latest_cell else None

            threads.append(
                Thread(
                    site_name=self.site_name,
                    subforum_id=subforum.subforum_id,
                    thread_id=thread_id,
                    title=title,
                    url=thread_url,
                    last_updated=last_updated,
                )
            )

        has_next = _has_next_page(soup)
        log.info(
            "parse_thread_list",
            extra={
                "subforum_id": subforum.subforum_id,
                "page": page,
                "thread_count": len(threads),
                "has_next": has_next,
            },
        )
        return threads, has_next

    async def get_posts(
        self, http: HttpClient, thread: Thread, page: int = 1
    ) -> tuple[list[Post], bool]:
        # On first page, also fetch threadmarks
        if page == 1:
            await self._fetch_threadmarks(http, thread)

        url = thread.url
        if page > 1:
            url = url.rstrip("/") + f"/page-{page}"

        resp = await http.get(url)
        soup = BeautifulSoup(resp.content, "lxml")

        threadmark_ids = self._threadmarks.get(thread.thread_id, set())
        posts: list[Post] = []

        for article in soup.select("article.message[data-content]"):
            data_content = str(article.get("data-content", ""))
            m = _POST_ID_RE.match(data_content)
            if m is None:
                continue
            post_id = m.group(1)

            author = str(article.get("data-author", ""))

            # Content: raw HTML from .message-body .bbWrapper
            body_el = article.select_one(".message-body .bbWrapper")
            content = str(body_el) if body_el else ""

            posted_at = _parse_time(article)

            # Ordinal: extract from the post number link (#N)
            ordinal = _extract_ordinal(article)

            is_story = post_id in threadmark_ids

            posts.append(
                Post(
                    site_name=self.site_name,
                    post_id=post_id,
                    container_id=thread.thread_id,
                    author=author,
                    content=content,
                    posted_at=posted_at,
                    is_story_post=is_story,
                    ordinal=ordinal,
                )
            )

        has_next = _has_next_page(soup)
        log.info(
            "parse_posts",
            extra={
                "thread_id": thread.thread_id,
                "page": page,
                "post_count": len(posts),
            },
        )
        return posts, has_next

    def identify_story_posts(self, posts: list[Post]) -> list[Post]:
        # Story posts are already marked in get_posts via threadmarks.
        # If no threadmarks were found, fall back to default (first post).
        if posts and not any(p.is_story_post for p in posts):
            posts[0].is_story_post = True
        return posts

    async def _fetch_threadmarks(self, http: HttpClient, thread: Thread) -> None:
        """Fetch the threadmarks index page and cache post IDs."""
        url = thread.url.rstrip("/") + "/threadmarks"
        try:
            resp = await http.get(url)
        except Exception:
            log.warning("threadmarks_failed", extra={"thread_id": thread.thread_id})
            return

        soup = BeautifulSoup(resp.content, "lxml")
        post_ids: set[str] = set()

        # Threadmark items are in structItem--threadmark containers
        for item in soup.select(".structItem--threadmark"):
            for link in item.select("a[href]"):
                href = str(link.get("href", ""))
                m = re.search(r"(?:#post-|/posts/)(\d+)", href)
                if m:
                    post_ids.add(m.group(1))
                    break  # first matching link per item is enough

        self._threadmarks[thread.thread_id] = post_ids
        log.info(
            "threadmarks_fetched",
            extra={"thread_id": thread.thread_id, "count": len(post_ids)},
        )


def _extract_thread_id(item: Tag) -> str | None:
    """Extract thread ID from class like 'js-threadListItem-12345'."""
    classes = item.get("class", [])
    for cls in classes:
        m = _THREAD_LIST_ITEM_RE.match(str(cls))
        if m:
            return m.group(1)
    # Fallback: extract from the title link URL
    link = item.select_one(".structItem-title a[href]")
    if link:
        href = str(link.get("href", ""))
        m = _THREAD_URL_ID_RE.search(href)
        if m:
            return m.group(1)
    return None


def _extract_node_id(node: Tag) -> str | None:
    """Extract numeric ID from a node element's CSS classes."""
    classes = node.get("class", [])
    for cls in classes:
        m = _NODE_ID_RE.match(str(cls))
        if m:
            return m.group(1)
    return None


def _parse_time(container: Tag) -> datetime | None:
    """Extract datetime from first <time datetime="..."> in a container."""
    time_el = container.select_one("time[datetime]")
    if time_el is None:
        return None
    dt_str = str(time_el.get("datetime", ""))
    try:
        return datetime.fromisoformat(dt_str).replace(tzinfo=timezone.utc)
    except ValueError:
        return None


def _has_next_page(soup: BeautifulSoup) -> bool:
    """Check if there's a next page link in XenForo pagination."""
    return soup.select_one(".pageNav-jump--next") is not None


def _extract_ordinal(article: Tag) -> int:
    """Extract the post number (#N) from the post header."""
    for link in article.select(".message-attribution-opposite a"):
        text = link.get_text(strip=True)
        if text.startswith("#"):
            try:
                return int(text[1:].replace(",", ""))
            except ValueError:
                pass
    return 0
