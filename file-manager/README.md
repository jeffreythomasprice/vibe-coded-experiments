# file-manager

## Setup

```sh
bun install
```

## Code generation

JSON Schema files in `packages/schemas/schemas/` are the source of truth for all data shapes. After editing a `.json` schema, regenerate the TypeScript types and validators:

```sh
bun run --cwd packages/schemas generate
```

Generated files (`src/generated/`) are committed so the package is importable without a build step.

## Start the server

The server requires a Postgres database. A `docker-compose.yml` is provided for local development.

```sh
# Start Postgres
docker compose up -d

# Run DB migrations (idempotent — safe to run on every startup)
bun run --cwd packages/server db:migrate

# Start the server
bun run --cwd packages/server start

# Start the server in watch mode (auto-restarts on file changes)
bun run --cwd packages/server dev
```

The server runs migrations automatically on startup (`src/index.ts`), so the manual `db:migrate` step above is only needed if you want to migrate without starting the server (e.g. in CI or before a deploy).

Database connection settings come from `packages/server/config/<NODE_ENV>.ts` (defaults to `development`). Edit that file to change the connection URL.

Default port: `8000`. Override with `PORT=<n>`.

### Stopping Postgres

```sh
docker compose down        # stop (data persists in the pgdata volume)
docker compose down -v     # stop and delete all data
```

### Inspecting the database

```sh
# Open a psql shell inside the running container
docker exec -it file-manager-postgres-1 psql -U filemanager -d filemanager
```

Useful psql commands once connected:

```sql
-- List all tables
\dt

-- Show schema for a table
\d provider_mounts

-- View all registered mounts
SELECT * FROM provider_mounts;

-- View mounts with pretty-printed config JSON
SELECT mount_id, scheme, config, created_at FROM provider_mounts ORDER BY created_at;

-- Count mounts by scheme
SELECT scheme, COUNT(*) FROM provider_mounts GROUP BY scheme;

-- Quit
\q
```

## Web UI

```sh
bun run --cwd packages/web dev
```

Opens at `http://localhost:8001`. The dev server proxies `/api` requests to the server at `http://localhost:8000`, so start the server first.

For a production build:

```sh
bun run --cwd packages/web build   # outputs to packages/web/dist/
```

## CLI

```sh
alias fm="bun run packages/cli/src/index.ts"
```

Override server URL with `FILE_MANAGER_API_URL=http://localhost:8000`.

### Providers

A **mount** registers a storage backend under a short name (`mountId`) that you then use to address files. For a local filesystem mount, `rootDir` is the absolute path on the machine running the server.

```sh
# Mount a local directory as 'docs'
# --scheme local    → use the local filesystem provider
# --config rootDir= → absolute path the server will expose (required for local)
fm providers mount docs --scheme local --config rootDir=/home/alice/documents

# Another example: expose /tmp/scratch as 'scratch'
fm providers mount scratch --scheme local --config rootDir=/tmp/scratch

# List all active mounts
fm providers list

# Unmount when done
fm providers unmount docs
```

### Files

URIs are in the form `<mountId>:/path` where `mountId` is the name you gave when mounting.

```sh
fm ls docs:/                       # list root of the 'docs' mount
fm stat docs:/reports/q1.pdf       # show metadata
fm cat docs:/notes.txt             # stream file to stdout
fm cp docs:/a.txt docs:/backup/a.txt
fm mv docs:/old.txt docs:/new.txt
fm rm docs:/unwanted.txt
fm mkdir docs:/new-folder

# Upload a local file to a mount
fm cp /local/path/file.txt docs:/file.txt
```

## Testing

```sh
# Unit + route tests (no external services required)
bun run test

# CLI integration tests — exercises the full stack end-to-end:
# CLI → HTTP client → Fastify routes → LocalProvider → filesystem
INTEGRATION=1 bun test packages/server/src/cli-integration.test.ts
```

The integration tests start their own server on a random port and clean up after themselves; no running server is required.
