# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Discord bot built with discord.js v14 and Bun. Logs all messages to the console with formatted output (timestamps, channel info, mention resolution). Includes an HTTP API for sending messages and a CLI client. Uses Bun's automatic `.env` loading for the `DISCORD_TOKEN`.

## Commands

- `bun run dev` — start with `--watch` for auto-reload
- `bun run start` — start without watch mode
- `bun run send "text"` — send a message via the HTTP API
- `bun install` — install dependencies
- `bun test` — run tests

## Architecture

- `src/index.ts` — Single entry point with subcommand dispatch (`serve` and `send`)
- `src/config.ts` — Zod config schema and loader (`discord-bot.config.json`)
- `src/schemas.ts` — Zod request validation schemas
- `src/events/` — Event handlers exported as `{ name, execute }` modules, registered via `client.on(event.name, event.execute)`

## Environment

Requires `DISCORD_TOKEN` in `.env` (loaded automatically by Bun).

## Key Details

- Uses discord.js `GatewayIntentBits` for Guilds, GuildMessages, DirectMessages, MessageContent, GuildMembers
- Uses `Partials.Channel` and `Partials.Message` for DM support
- Prefer Bun APIs over Node.js equivalents (see parent CLAUDE.md for full list)
