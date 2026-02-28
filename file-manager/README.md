# file-manager

## Setup

```sh
bun install
```

## Start the server

```sh
bun run --cwd packages/server start
```

Default port: `8000`. Override with `PORT=<n>`.

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
```
