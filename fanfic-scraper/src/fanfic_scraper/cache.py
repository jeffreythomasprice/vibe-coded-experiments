from __future__ import annotations

import json
import time
from datetime import datetime

import aiosqlite

from .models import Comment, Post, Story, Subforum, Thread

_SCHEMA = """
CREATE TABLE IF NOT EXISTS subforums (
    site_name TEXT NOT NULL,
    subforum_id TEXT NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    PRIMARY KEY (site_name, subforum_id)
);

CREATE TABLE IF NOT EXISTS threads (
    site_name TEXT NOT NULL,
    subforum_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    last_updated TEXT,
    PRIMARY KEY (site_name, thread_id)
);

CREATE TABLE IF NOT EXISTS posts (
    site_name TEXT NOT NULL,
    post_id TEXT NOT NULL,
    container_id TEXT NOT NULL,
    author TEXT NOT NULL,
    content TEXT NOT NULL,
    posted_at TEXT,
    is_story_post INTEGER DEFAULT 0,
    ordinal INTEGER DEFAULT 0,
    PRIMARY KEY (site_name, post_id)
);
CREATE INDEX IF NOT EXISTS idx_posts_container ON posts(site_name, container_id);
CREATE INDEX IF NOT EXISTS idx_posts_posted_at ON posts(posted_at);

CREATE TABLE IF NOT EXISTS stories (
    site_name TEXT NOT NULL,
    story_id TEXT NOT NULL,
    title TEXT NOT NULL,
    author TEXT NOT NULL,
    url TEXT NOT NULL,
    summary TEXT DEFAULT '',
    last_updated TEXT,
    PRIMARY KEY (site_name, story_id)
);

CREATE TABLE IF NOT EXISTS story_tags (
    site_name TEXT NOT NULL,
    story_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (site_name, story_id, tag)
);

CREATE TABLE IF NOT EXISTS story_categories (
    site_name TEXT NOT NULL,
    story_id TEXT NOT NULL,
    category TEXT NOT NULL,
    PRIMARY KEY (site_name, story_id, category)
);

CREATE TABLE IF NOT EXISTS comments (
    site_name TEXT NOT NULL,
    comment_id TEXT NOT NULL,
    story_id TEXT NOT NULL,
    author TEXT NOT NULL,
    content TEXT NOT NULL,
    posted_at TEXT,
    PRIMARY KEY (site_name, comment_id)
);
CREATE INDEX IF NOT EXISTS idx_comments_story ON comments(site_name, story_id);

CREATE TABLE IF NOT EXISTS http_cache (
    url TEXT PRIMARY KEY,
    status_code INTEGER NOT NULL,
    headers TEXT NOT NULL,
    body BLOB NOT NULL,
    fetched_at TEXT NOT NULL
);
"""


def _dt_to_iso(dt: datetime | None) -> str | None:
    return dt.isoformat() if dt else None


def _iso_to_dt(s: str | None) -> datetime | None:
    return datetime.fromisoformat(s) if s else None


class Cache:
    """SQLite cache for HTTP responses and parsed content."""

    def __init__(self, db_path: str = "fanfic_cache.db") -> None:
        self._db_path = db_path
        self._db: aiosqlite.Connection | None = None

    async def open(self) -> None:
        self._db = await aiosqlite.connect(self._db_path)
        self._db.row_factory = aiosqlite.Row
        await self._db.executescript(_SCHEMA)
        await self._db.commit()

    async def close(self) -> None:
        if self._db:
            await self._db.close()
            self._db = None

    async def __aenter__(self) -> Cache:
        await self.open()
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.close()

    # --- HTTP cache ---

    async def get_http(
        self, url: str, *, max_age: float | None = None
    ) -> tuple[int, dict[str, str], bytes] | None:
        assert self._db
        row = await self._db.execute_fetchall(
            "SELECT status_code, headers, body, fetched_at FROM http_cache WHERE url = ?", (url,)
        )
        if not row:
            return None
        r = row[0]
        if max_age is not None:
            fetched_at = datetime.fromisoformat(r[3])
            age = time.time() - fetched_at.timestamp()
            if age > max_age:
                return None
        return r[0], json.loads(r[1]), r[2]

    async def put_http(
        self, url: str, status: int, headers: dict[str, str], body: bytes
    ) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO http_cache (url, status_code, headers, body, fetched_at) "
            "VALUES (?, ?, ?, ?, ?)",
            (url, status, json.dumps(headers), body, datetime.now().isoformat()),
        )
        await self._db.commit()

    # --- Upserts ---

    async def upsert_subforum(self, sf: Subforum) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO subforums (site_name, subforum_id, name, url) "
            "VALUES (?, ?, ?, ?)",
            (sf.site_name, sf.subforum_id, sf.name, sf.url),
        )
        await self._db.commit()

    async def upsert_thread(self, t: Thread) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO threads (site_name, subforum_id, thread_id, title, url, last_updated) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (t.site_name, t.subforum_id, t.thread_id, t.title, t.url, _dt_to_iso(t.last_updated)),
        )
        await self._db.commit()

    async def upsert_post(self, p: Post) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO posts (site_name, post_id, container_id, author, content, posted_at, is_story_post, ordinal) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (p.site_name, p.post_id, p.container_id, p.author, p.content,
             _dt_to_iso(p.posted_at), int(p.is_story_post), p.ordinal),
        )
        await self._db.commit()

    async def upsert_many_posts(self, posts: list[Post]) -> None:
        assert self._db
        await self._db.executemany(
            "INSERT OR REPLACE INTO posts (site_name, post_id, container_id, author, content, posted_at, is_story_post, ordinal) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                (p.site_name, p.post_id, p.container_id, p.author, p.content,
                 _dt_to_iso(p.posted_at), int(p.is_story_post), p.ordinal)
                for p in posts
            ],
        )
        await self._db.commit()

    async def upsert_story(self, s: Story) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO stories (site_name, story_id, title, author, url, summary, last_updated) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (s.site_name, s.story_id, s.title, s.author, s.url, s.summary,
             _dt_to_iso(s.last_updated)),
        )
        # Replace tags and categories
        await self._db.execute(
            "DELETE FROM story_tags WHERE site_name = ? AND story_id = ?",
            (s.site_name, s.story_id),
        )
        if s.tags:
            await self._db.executemany(
                "INSERT INTO story_tags (site_name, story_id, tag) VALUES (?, ?, ?)",
                [(s.site_name, s.story_id, tag) for tag in s.tags],
            )
        await self._db.execute(
            "DELETE FROM story_categories WHERE site_name = ? AND story_id = ?",
            (s.site_name, s.story_id),
        )
        if s.categories:
            await self._db.executemany(
                "INSERT INTO story_categories (site_name, story_id, category) VALUES (?, ?, ?)",
                [(s.site_name, s.story_id, cat) for cat in s.categories],
            )
        await self._db.commit()

    async def upsert_comment(self, c: Comment) -> None:
        assert self._db
        await self._db.execute(
            "INSERT OR REPLACE INTO comments (site_name, comment_id, story_id, author, content, posted_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (c.site_name, c.comment_id, c.story_id, c.author, c.content,
             _dt_to_iso(c.posted_at)),
        )
        await self._db.commit()

    async def upsert_many_comments(self, comments: list[Comment]) -> None:
        assert self._db
        await self._db.executemany(
            "INSERT OR REPLACE INTO comments (site_name, comment_id, story_id, author, content, posted_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            [
                (c.site_name, c.comment_id, c.story_id, c.author, c.content,
                 _dt_to_iso(c.posted_at))
                for c in comments
            ],
        )
        await self._db.commit()

    # --- Queries ---

    async def get_subforums(self, site_name: str) -> list[Subforum]:
        assert self._db
        rows = await self._db.execute_fetchall(
            "SELECT site_name, subforum_id, name, url FROM subforums WHERE site_name = ?",
            (site_name,),
        )
        return [Subforum(site_name=r[0], subforum_id=r[1], name=r[2], url=r[3]) for r in rows]

    async def get_threads(
        self,
        site_name: str,
        subforum_id: str | None = None,
        updated_after: datetime | None = None,
        updated_before: datetime | None = None,
    ) -> list[Thread]:
        assert self._db
        sql = "SELECT site_name, subforum_id, thread_id, title, url, last_updated FROM threads WHERE site_name = ?"
        params: list[str] = [site_name]
        if subforum_id is not None:
            sql += " AND subforum_id = ?"
            params.append(subforum_id)
        if updated_after is not None:
            sql += " AND last_updated >= ?"
            params.append(updated_after.isoformat())
        if updated_before is not None:
            sql += " AND last_updated <= ?"
            params.append(updated_before.isoformat())
        sql += " ORDER BY last_updated DESC"
        rows = await self._db.execute_fetchall(sql, params)
        return [
            Thread(
                site_name=r[0], subforum_id=r[1], thread_id=r[2],
                title=r[3], url=r[4], last_updated=_iso_to_dt(r[5]),
            )
            for r in rows
        ]

    async def get_posts(
        self,
        site_name: str,
        container_id: str,
        posted_after: datetime | None = None,
        posted_before: datetime | None = None,
        story_only: bool = False,
    ) -> list[Post]:
        assert self._db
        sql = "SELECT site_name, post_id, container_id, author, content, posted_at, is_story_post, ordinal FROM posts WHERE site_name = ? AND container_id = ?"
        params: list[str | int] = [site_name, container_id]
        if posted_after is not None:
            sql += " AND posted_at >= ?"
            params.append(posted_after.isoformat())
        if posted_before is not None:
            sql += " AND posted_at <= ?"
            params.append(posted_before.isoformat())
        if story_only:
            sql += " AND is_story_post = 1"
        sql += " ORDER BY ordinal"
        rows = await self._db.execute_fetchall(sql, params)
        return [
            Post(
                site_name=r[0], post_id=r[1], container_id=r[2],
                author=r[3], content=r[4], posted_at=_iso_to_dt(r[5]),
                is_story_post=bool(r[6]), ordinal=r[7],
            )
            for r in rows
        ]

    async def get_stories(
        self,
        site_name: str,
        categories: list[str] | None = None,
        tags: list[str] | None = None,
        updated_after: datetime | None = None,
    ) -> list[Story]:
        assert self._db
        sql = "SELECT DISTINCT s.site_name, s.story_id, s.title, s.author, s.url, s.summary, s.last_updated FROM stories s"
        joins: list[str] = []
        wheres = ["s.site_name = ?"]
        params: list[str] = [site_name]

        if categories:
            joins.append("JOIN story_categories sc ON s.site_name = sc.site_name AND s.story_id = sc.story_id")
            placeholders = ", ".join("?" for _ in categories)
            wheres.append(f"sc.category IN ({placeholders})")
            params.extend(categories)

        if tags:
            joins.append("JOIN story_tags st ON s.site_name = st.site_name AND s.story_id = st.story_id")
            placeholders = ", ".join("?" for _ in tags)
            wheres.append(f"st.tag IN ({placeholders})")
            params.extend(tags)

        if updated_after is not None:
            wheres.append("s.last_updated >= ?")
            params.append(updated_after.isoformat())

        full_sql = sql + " " + " ".join(joins) + " WHERE " + " AND ".join(wheres) + " ORDER BY s.last_updated DESC"
        rows = await self._db.execute_fetchall(full_sql, params)

        stories = []
        for r in rows:
            # Fetch tags and categories for each story
            tag_rows = await self._db.execute_fetchall(
                "SELECT tag FROM story_tags WHERE site_name = ? AND story_id = ?",
                (r[0], r[1]),
            )
            cat_rows = await self._db.execute_fetchall(
                "SELECT category FROM story_categories WHERE site_name = ? AND story_id = ?",
                (r[0], r[1]),
            )
            stories.append(Story(
                site_name=r[0], story_id=r[1], title=r[2], author=r[3],
                url=r[4], summary=r[5], last_updated=_iso_to_dt(r[6]),
                tags=[t[0] for t in tag_rows],
                categories=[c[0] for c in cat_rows],
            ))
        return stories

    async def get_comments(self, site_name: str, story_id: str) -> list[Comment]:
        assert self._db
        rows = await self._db.execute_fetchall(
            "SELECT site_name, comment_id, story_id, author, content, posted_at "
            "FROM comments WHERE site_name = ? AND story_id = ? ORDER BY posted_at",
            (site_name, story_id),
        )
        return [
            Comment(
                site_name=r[0], comment_id=r[1], story_id=r[2],
                author=r[3], content=r[4], posted_at=_iso_to_dt(r[5]),
            )
            for r in rows
        ]
