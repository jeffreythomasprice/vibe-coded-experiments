"""Structured JSON logging for fanfic-scraper."""

from __future__ import annotations

import json
import logging
import sys
from datetime import datetime, timezone


class JsonFormatter(logging.Formatter):
    """Formats log records as single-line JSON objects."""

    def format(self, record: logging.LogRecord) -> str:
        entry: dict = {
            "ts": datetime.fromtimestamp(record.created, tz=timezone.utc).isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "msg": record.getMessage(),
        }
        # Merge any extra keys that aren't standard LogRecord attributes
        for key, value in vars(record).items():
            if key not in _STANDARD_ATTRS and key not in entry:
                entry[key] = value
        return json.dumps(entry, default=str)


class _BriefFormatter(logging.Formatter):
    """Minimal one-line format for stderr."""

    def format(self, record: logging.LogRecord) -> str:
        ts = datetime.fromtimestamp(record.created, tz=timezone.utc).strftime("%H:%M:%S")
        extra_parts: list[str] = []
        for key, value in vars(record).items():
            if key not in _STANDARD_ATTRS and key not in ("msg", "message"):
                extra_parts.append(f"{key}={value}")
        extra = " " + " ".join(extra_parts) if extra_parts else ""
        return f"{ts} {record.levelname[0]} {record.name}: {record.getMessage()}{extra}"


# Standard LogRecord attributes to exclude from extra fields
_STANDARD_ATTRS = frozenset(vars(logging.LogRecord("", 0, "", 0, "", (), None)).keys())


def setup_logging(log_path: str | None = None, *, verbose: bool = False) -> str:
    """Configure structured logging.

    Args:
        log_path: Path for the JSON log file. Defaults to
            ``/tmp/fanfic-scraper-{date}.jsonl``.
        verbose: If True, set stderr handler to DEBUG (same as file).

    Returns:
        The resolved log file path.
    """
    if log_path is None:
        date_str = datetime.now(tz=timezone.utc).strftime("%Y%m%d")
        log_path = f"/tmp/fanfic-scraper-{date_str}.jsonl"

    root = logging.getLogger()
    root.setLevel(logging.DEBUG)

    # JSON file handler — captures everything (DEBUG+)
    file_handler = logging.FileHandler(log_path)
    file_handler.setLevel(logging.DEBUG)
    file_handler.setFormatter(JsonFormatter())
    root.addHandler(file_handler)

    # Stderr handler — human-readable
    stderr_handler = logging.StreamHandler(sys.stderr)
    stderr_handler.setLevel(logging.DEBUG if verbose else logging.INFO)
    stderr_handler.setFormatter(_BriefFormatter())
    root.addHandler(stderr_handler)

    # Suppress noisy third-party debug logs
    logging.getLogger("aiosqlite").setLevel(logging.WARNING)
    logging.getLogger("httpx").setLevel(logging.WARNING)
    logging.getLogger("httpcore").setLevel(logging.WARNING)

    print(f"Logging to {log_path}")

    return log_path
