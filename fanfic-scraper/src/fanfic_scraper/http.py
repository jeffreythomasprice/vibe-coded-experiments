from __future__ import annotations

import asyncio
import json
import logging
import time
from urllib.parse import urlparse

import httpx

from .cache import Cache

log = logging.getLogger(__name__)


class _HostThrottle:
    __slots__ = ("semaphore", "last_request", "min_delay")

    def __init__(self, max_concurrent: int, min_delay: float) -> None:
        self.semaphore = asyncio.Semaphore(max_concurrent)
        self.last_request: float = 0.0
        self.min_delay = min_delay


class HttpClient:
    """Async HTTP client with per-host concurrency limits and retry logic."""

    def __init__(
        self,
        *,
        max_concurrent_per_host: int = 2,
        min_delay_per_host: float = 1.0,
        max_retries: int = 3,
        user_agent: str = "fanfic-scraper/0.1 (polite bot)",
        cache: Cache | None = None,
    ) -> None:
        self._max_concurrent = max_concurrent_per_host
        self._min_delay = min_delay_per_host
        self._max_retries = max_retries
        self._user_agent = user_agent
        self._cache = cache
        self._throttles: dict[str, _HostThrottle] = {}
        self._client: httpx.AsyncClient | None = None

    async def __aenter__(self) -> HttpClient:
        self._client = httpx.AsyncClient(
            headers={"User-Agent": self._user_agent},
            follow_redirects=True,
            timeout=30.0,
        )
        return self

    async def __aexit__(self, *exc: object) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None

    def _get_throttle(self, url: str) -> _HostThrottle:
        host = urlparse(url).netloc
        if host not in self._throttles:
            self._throttles[host] = _HostThrottle(self._max_concurrent, self._min_delay)
        return self._throttles[host]

    async def get(
        self, url: str, *, use_cache: bool = True, max_age: float | None = None
    ) -> httpx.Response:
        """Fetch a URL, respecting cache, per-host concurrency, and retry logic."""
        # Check cache first
        if use_cache and self._cache:
            cached = await self._cache.get_http(url, max_age=max_age)
            if cached is not None:
                status, headers, body = cached
                # Strip encoding headers — cached body is already decoded
                headers = {
                    k: v for k, v in headers.items()
                    if k.lower() not in ("content-encoding", "content-length")
                }
                log.debug("cache_hit", extra={"url": url, "status": status})
                return httpx.Response(
                    status_code=status,
                    headers=headers,
                    content=body,
                    request=httpx.Request("GET", url),
                )

        log.debug("cache_miss", extra={"url": url})
        throttle = self._get_throttle(url)
        last_exc: Exception | None = None

        for attempt in range(self._max_retries + 1):
            try:
                log.info("http_fetch", extra={"url": url, "attempt": attempt + 1})
                t0 = time.monotonic()
                response = await self._do_request(url, throttle)
                elapsed_ms = (time.monotonic() - t0) * 1000
                log.info(
                    "http_response",
                    extra={"url": url, "status": response.status_code, "elapsed_ms": round(elapsed_ms)},
                )

                if response.status_code == 429:
                    retry_after = _parse_retry_after(response)
                    log.warning("http_429", extra={"url": url, "retry_after": retry_after})
                    await asyncio.sleep(retry_after)
                    continue

                if response.status_code >= 500:
                    backoff = 2**attempt
                    log.warning("http_5xx", extra={"url": url, "status": response.status_code, "backoff": backoff})
                    await asyncio.sleep(backoff)
                    continue

                response.raise_for_status()

                # Cache successful response
                if self._cache:
                    await self._cache.put_http(
                        url,
                        response.status_code,
                        dict(response.headers),
                        response.content,
                    )
                    log.debug("cache_store", extra={"url": url, "status": response.status_code})

                return response

            except httpx.HTTPStatusError:
                raise
            except httpx.HTTPError as exc:
                last_exc = exc
                backoff = 2**attempt
                log.error("http_error", extra={"url": url, "error": str(exc), "backoff": backoff})
                await asyncio.sleep(backoff)

        if last_exc:
            raise last_exc
        raise RuntimeError(f"Max retries exceeded for {url}")

    async def _do_request(self, url: str, throttle: _HostThrottle) -> httpx.Response:
        assert self._client is not None
        async with throttle.semaphore:
            # Enforce minimum delay between requests to same host
            now = time.monotonic()
            elapsed = now - throttle.last_request
            if elapsed < throttle.min_delay:
                await asyncio.sleep(throttle.min_delay - elapsed)
            throttle.last_request = time.monotonic()
            return await self._client.get(url)


def _parse_retry_after(response: httpx.Response) -> float:
    """Parse Retry-After header, defaulting to 60s if missing or unparseable."""
    value = response.headers.get("retry-after", "")
    try:
        return float(value)
    except (ValueError, TypeError):
        return 60.0
