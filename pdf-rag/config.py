"""Central configuration — reads from .env file, exits if required vars are missing."""

import os
import sys
from dotenv import load_dotenv
import httpx

load_dotenv()

REQUIRED_ENV_VARS = [
    "OLLAMA_BASE_URL",
    "EMBED_MODEL",
    "CHAT_MODEL",
    "DB_DSN",
    "CHUNK_SIZE",
    "CHUNK_OVERLAP",
    "CONTEXT_WINDOW",
]

_missing = [v for v in REQUIRED_ENV_VARS if not os.getenv(v)]
if _missing:
    print(
        f"Error: missing required environment variables: {', '.join(_missing)}\n"
        "Set them in your .env file or environment before running.",
        file=sys.stderr,
    )
    sys.exit(1)

# --- Ollama / LiteLLM --------------------------------------------------
OLLAMA_BASE_URL = os.environ["OLLAMA_BASE_URL"]
EMBED_MODEL = os.environ["EMBED_MODEL"]
CHAT_MODEL = os.environ["CHAT_MODEL"]


def _ensure_ollama_model(model: str) -> None:
    """Pull an ollama model if it isn't already available locally."""
    # Strip the "ollama/" prefix that LiteLLM uses for routing
    if not model.startswith("ollama/"):
        return
    ollama_name = model[len("ollama/"):]

    try:
        resp = httpx.get(f"{OLLAMA_BASE_URL}/api/tags", timeout=10)
        resp.raise_for_status()
        local_models = [m["name"] for m in resp.json().get("models", [])]
    except httpx.HTTPError as exc:
        print(f"Warning: could not check ollama models ({exc})", file=sys.stderr)
        return

    # Match with or without a :latest tag
    if any(m == ollama_name or m == f"{ollama_name}:latest" or m.split(":")[0] == ollama_name for m in local_models):
        return

    print(f"Model '{ollama_name}' not found locally — pulling from ollama…")
    try:
        with httpx.stream(
            "POST",
            f"{OLLAMA_BASE_URL}/api/pull",
            json={"name": ollama_name},
            timeout=None,
        ) as stream:
            for line in stream.iter_lines():
                if line:
                    import json
                    data = json.loads(line)
                    status = data.get("status", "")
                    if "pulling" in status or "verifying" in status or "writing" in status:
                        total = data.get("total", 0)
                        completed = data.get("completed", 0)
                        if total:
                            pct = completed / total * 100
                            print(f"\r  {status}: {pct:.0f}%", end="", flush=True)
                        else:
                            print(f"\r  {status}…", end="", flush=True)
                    elif status == "success":
                        print(f"\n  Model '{ollama_name}' pulled successfully.")
    except httpx.HTTPError as exc:
        print(f"\nError: failed to pull model '{ollama_name}': {exc}", file=sys.stderr)
        sys.exit(1)


_ensure_ollama_model(EMBED_MODEL)
_ensure_ollama_model(CHAT_MODEL)

# --- PostgreSQL ---------------------------------------------------------
DB_DSN = os.environ["DB_DSN"]

# --- Chunking -----------------------------------------------------------
CHUNK_SIZE = int(os.environ["CHUNK_SIZE"])       # characters
CHUNK_OVERLAP = int(os.environ["CHUNK_OVERLAP"])   # characters
CONTEXT_WINDOW = int(os.environ["CONTEXT_WINDOW"])  # chunks before/after match
