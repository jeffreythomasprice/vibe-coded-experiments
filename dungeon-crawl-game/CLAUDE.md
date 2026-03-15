# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

AI-powered dungeon crawl game inspired by "Betrayal at House on the Hill." Uses LLM (Ollama) to procedurally generate rooms, events, and items. Victorian gothic haunted mansion theme.

## Commands

```bash
bun install                # install dependencies
bun run generate           # codegen: JSON schemas → TypeScript types + validators
bun run src/index.ts       # run the game (requires Ollama running locally)
bun test                   # run tests
```

## Architecture

**Schema-driven development:** JSON schemas in `schemas/` are the single source of truth for all domain types. Run `bun run generate` after changing any schema — this produces `src/generated/` (types, AJV validators, re-exports).

**Key modules:**
- `src/index.ts` — entry point: loads config + theme, generates a room via LLM
- `src/generate-room.ts` — Strands SDK agent with OpenAI-compatible client pointed at Ollama; uses structured output constrained to Room schema
- `src/config.ts` — loads `config.yaml` (Ollama model name), validates with AJV
- `src/theme.ts` — loads `assets/theme.yaml` (tagged context entries fed to LLM system prompt)
- `scripts/generate.ts` — codegen script reading `schemas/*.json`, writing `src/generated/`

**Validation pattern:** `assertX(data)` throws on invalid, `isX(data): data is X` returns boolean. Schemas with cross-references (e.g., room.json → effect.json) are pre-registered by filename in the codegen.

**Domain types:** Config, GeneratorContext, Room, RoomEvent, Effect, Item, Player (GURPS-style stats: strength/dexterity/intelligence/health, default 10).

**Config files:** `config.yaml` (runtime — Ollama model), `assets/theme.yaml` (game theme data).

## Bun

Default to Bun for everything. Use `bun` instead of `node`/`npm`/`yarn`. Bun auto-loads `.env`.

Prefer Bun built-ins: `Bun.serve()` over express, `bun:sqlite` over better-sqlite3, `Bun.file` over `node:fs`, `Bun.$` over execa.

## Code Style

Prefer no comments or terse comments. Don't use `!` to ignore nullable errors — handle `| null` / `| undefined` explicitly.
