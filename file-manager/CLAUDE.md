# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
bun install                                    # install all workspace deps
bun run build                                  # turbo build (all packages)
bun run dev                                    # turbo dev --parallel (all packages, watch mode)
bun run test                                   # turbo test (all packages)

bun run --cwd packages/schemas generate        # (re)generate types + validators from schemas/*.json

bun run --cwd packages/server start            # start API server (default port 8000)
bun run --cwd packages/server dev              # start with --watch

bun run packages/cli/src/index.ts <command>    # run CLI directly
alias fm="bun run packages/cli/src/index.ts"  # convenience alias

# Tests are co-located (src/foo.ts → src/foo.test.ts), run with bun test:
bun test packages/server/src/routes/files.test.ts   # single test file
```

Environment variables: `PORT`, `HOST` for the server; `FILE_MANAGER_API_URL` for the CLI (default `http://localhost:8000`).

## Architecture

Bun monorepo with Turborepo. Four active packages:

- **`@file-manager/schemas`** — JSON Schema source of truth + generated TypeScript types and AJV validators.
- **`@file-manager/shared`** — pure type definitions, no runtime code. Every other package imports from here.
- **`@file-manager/server`** — Fastify REST API. All business logic and provider implementations live here.
- **`@file-manager/cli`** — Commander CLI. Thin HTTP client only; imports `shared` but never `server`.

`web` and sync engine are designed (see `ARCHITECTURE.md`) but not yet implemented.

### Schema pipeline

`packages/schemas` is the single source of truth for all wire-format data shapes.

- **Source**: `packages/schemas/schemas/*.json` — hand-authored JSON Schema (draft-07) files.
- **Codegen**: `scripts/generate.ts` reads every `.json` and writes three output kinds into `src/generated/`:
  - `types/<name>.ts` — TypeScript interfaces via `json-schema-to-typescript`
  - `validators/<name>.ts` — `isX(data: unknown): data is X` and `assertX(data)` wrappers using AJV
  - `schemas/<name>.ts` — `xSchema as const` objects for Fastify's `schema:` option
- **Generated files are committed.** Re-run the generator whenever a `.json` schema changes; diffs are visible in PRs.
- The Turbo `generate` task runs before `build` in every package that depends on it.

Adding a new schema: drop a `kebab-case.json` file into `schemas/` and re-run `generate`. No other changes required.

### StorageProvider: the core extension point

`packages/shared/src/index.ts` defines the `StorageProvider` interface. Adding a new backend (S3, SFTP, etc.) means:
1. Implement `StorageProvider` in `packages/server/src/providers/<name>.ts`
2. Register it in `packages/server/src/provider-registry.ts`

No other code changes required. `LocalProvider` (`providers/local.ts`) is the reference implementation.

### Provider URI scheme

All file addresses use `<scheme>://<mountId>/<path>` (e.g. `local://docs/reports/q1.pdf`). A **mount** registers a `StorageProvider` instance under a user-chosen `mountId`. The server's `ProviderRegistry` maps `mountId → ProviderMount` at runtime (in-memory, not persisted).

### Streaming

`StorageProvider.read()` returns `AsyncIterable<Buffer>`; `write()` accepts `AsyncIterable<Buffer>`. Files are never fully buffered. Cross-mount moves stream directly: `destProvider.write(destPath, srcProvider.read(srcPath))` then delete source.

### Fastify gotchas

- **Binary uploads**: `server.ts` registers an `application/octet-stream` content-type parser (`parseAs: 'buffer'`). Route handlers receive `req.body` as `Buffer`.
- **DELETE with no body**: the fetch wrapper in `packages/cli/src/api/client.ts` only sets `Content-Type: application/json` when `options.body` is present. Sending the header on an empty-body DELETE causes Fastify to reject with 400.

### Path safety

`LocalProvider.resolve()` validates that the resolved absolute path stays within `rootDir`. Any path traversal attempt throws before any filesystem operation.

## TypeScript

All packages extend `tsconfig.base.json` with strict mode plus `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `noImplicitOverride`, `noImplicitReturns`, and `verbatimModuleSyntax`. `skipLibCheck: false`.

## ESLint

`.then()` / `.catch()` chains are forbidden — use `async`/`await` exclusively. Config uses `@typescript-eslint/recommended-type-checked` + `strict-type-checked`.

## Testing

`bun test` (vitest-compatible API, no extra dependency). Test files co-located with source. Integration tests requiring external services are gated behind an env var and skipped in CI unless the service is available.
