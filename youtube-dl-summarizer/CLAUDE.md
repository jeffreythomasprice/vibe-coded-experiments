# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

CLI tool that downloads a YouTube video, transcribes it, and summarizes it using Claude. Installed as `yt-summarize`.

## Commands

```sh
bun src/index.ts <youtube-url>              # run directly
bun src/index.ts summarize <url> -v         # verbose mode
bun src/index.ts summarize <url> --no-snapshots  # skip frame extraction
bun test                                    # run all tests
bun test src/utils/parse-captions.test.ts   # run a single test file
```

## Setup

Copy `.env.template` to `.env` and set `ANTHROPIC_API_KEY`. Bun loads `.env` automatically.

### External binaries required

- **yt-dlp** — video/caption download
- **ffmpeg** — audio extraction + frame extraction from video
- **whisper.cpp** — speech-to-text (only needed when no captions available). Set `WHISPER_BINARY` and `WHISPER_MODEL` env vars.

## Architecture

### Pipeline (`src/pipeline.ts`)

Sequential steps orchestrated by `runPipeline()`. Each step is cached — re-running skips completed work.

1. **Download** (`steps/download.ts`) — yt-dlp fetches video + optional VTT captions
2. **Transcript** — if captions exist, parse VTT (`utils/parse-captions.ts`); otherwise extract audio (`steps/extract-audio.ts`) then transcribe via whisper.cpp (`steps/transcribe.ts`)
3. **Summarize** (`steps/summarize.ts`) — sends transcript to the Anthropic API via `@anthropic-ai/sdk`, produces markdown with `[HH:MM:SS]` timestamps
4. **Extract frames** (`steps/extract-frames.ts`) — ffmpeg grabs screenshots at each timestamp from the summary (max 15)
5. **Enrich** (`steps/enrich-summary.ts`) — inserts markdown image references below timestamp lines in the summary

### Cache (`src/cache.ts`)

All intermediate artifacts are stored in `~/.cache/yt-summarizer/<videoId>/`. Cache keys are defined in `CACHE_FILES`. Each step checks `cacheExists()` before doing work.

### Subprocess (`src/utils/subprocess.ts`)

- `ensureBinary(name)` — checks PATH via `Bun.which()`, throws with install hint if missing
- `run(command)` — wraps `Bun.spawn`, captures stdout/stderr, throws on non-zero exit

## Bun conventions

- Use `bun` instead of `node`/`npm`/`yarn`
- Use `Bun.file()` / `Bun.write()` for file I/O
- Use `Bun.spawn()` for subprocesses (wrapped in `src/utils/subprocess.ts`)
- Bun loads `.env` automatically — no dotenv
- Tests use `bun:test` (`import { test, expect } from "bun:test"`)
