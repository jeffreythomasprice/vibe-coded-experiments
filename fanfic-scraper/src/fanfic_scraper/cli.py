"""CLI utilities: duration parsing and terminal spinner."""

from __future__ import annotations

import asyncio
import re
import sys
from contextlib import asynccontextmanager
from datetime import timedelta
from typing import AsyncIterator

_DURATION_RE = re.compile(
    r"^(?:(\d+)d)?(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$"
)

_UNIT_SECONDS = {
    "s": 1,
    "m": 60,
    "h": 3600,
    "d": 86400,
    "w": 604800,
}


def parse_duration(s: str) -> timedelta:
    """Parse a human-readable duration like '12h', '7d', '2d12h', '30m'.

    Supported units: s, m, h, d, w.  Bare number treated as seconds.
    """
    s = s.strip().lower()

    # Single-unit shorthand: "12h", "7d", "30s"
    if s and s[-1] in _UNIT_SECONDS:
        try:
            value = int(s[:-1])
            return timedelta(seconds=value * _UNIT_SECONDS[s[-1]])
        except ValueError:
            pass

    # Compound: "2d12h", "1h30m"
    m = _DURATION_RE.match(s)
    if m and any(m.groups()):
        days = int(m.group(1) or 0)
        hours = int(m.group(2) or 0)
        minutes = int(m.group(3) or 0)
        seconds = int(m.group(4) or 0)
        return timedelta(days=days, hours=hours, minutes=minutes, seconds=seconds)

    raise ValueError(
        f"Invalid duration: {s!r}. Use e.g. '12h', '7d', '2d12h', '30m'."
    )


_SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]


@asynccontextmanager
async def spinner(message: str) -> AsyncIterator[None]:
    """Show a CLI-safe spinner on stderr while the body runs."""
    stop = asyncio.Event()
    is_tty = sys.stderr.isatty()

    async def _animate() -> None:
        i = 0
        while not stop.is_set():
            frame = _SPINNER_FRAMES[i % len(_SPINNER_FRAMES)]
            if is_tty:
                sys.stderr.write(f"\r{frame} {message}")
                sys.stderr.flush()
            try:
                await asyncio.wait_for(stop.wait(), timeout=0.08)
            except asyncio.TimeoutError:
                pass
            i += 1
        if is_tty:
            sys.stderr.write(f"\r\033[2K")  # clear the line
            sys.stderr.flush()

    task = asyncio.create_task(_animate())
    try:
        yield
    finally:
        stop.set()
        await task
