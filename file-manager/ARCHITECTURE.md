# File Manager — Architecture

## 1. Overview

A backend-agnostic, multi-provider file manager with real-time sync, a REST/WebSocket server, a CLI, and a dual-pane web UI.

**Guiding principles:**

- **Backend-agnostic via `StorageProvider`** — every storage backend (local filesystem, S3, Google Drive, SFTP, etc.) implements the same interface. The rest of the system never touches provider-specific APIs directly.
- **Thin clients** — both the CLI and the web frontend are pure HTTP/WebSocket consumers. All business logic lives in the server.
- **Strict TypeScript throughout** — every package uses `strict: true` plus additional strictness flags. Type safety is non-negotiable.
- **Streaming by default** — file reads and writes use `AsyncIterable<Buffer>` to avoid loading files into memory.

---

## 2. Monorepo Structure

Managed with **bun workspaces** + **turborepo**.

```
file-manager/
├── package.json              # root workspace (bun workspaces)
├── bunfig.toml               # bun configuration
├── turbo.json                # turborepo pipeline
├── tsconfig.base.json        # shared TypeScript base config
├── .eslintrc.cjs             # root ESLint config
├── .prettierrc.json          # prettier config
└── packages/
    ├── shared/               # @file-manager/shared
    │   ├── package.json
    │   ├── tsconfig.json
    │   └── src/
    │       └── index.ts      # all shared types + StorageProvider interface
    │
    ├── server/               # @file-manager/server
    │   ├── package.json
    │   ├── tsconfig.json
    │   └── src/
    │       ├── index.ts
    │       ├── server.ts
    │       ├── provider-registry.ts
    │       ├── routes/
    │       │   ├── files.ts
    │       │   ├── providers.ts
    │       │   └── sync.ts
    │       ├── ws/
    │       │   └── events.ts
    │       ├── sync/
    │       │   ├── engine.ts
    │       │   ├── job-runner.ts
    │       │   ├── event-queue.ts
    │       │   ├── conflict-resolver.ts
    │       │   ├── snapshot-store.ts
    │       │   └── change-applier.ts
    │       └── providers/
    │           └── local.ts
    │
    ├── cli/                  # @file-manager/cli
    │   ├── package.json
    │   ├── tsconfig.json
    │   └── src/
    │       ├── index.ts      # binary entry point (bin: "files")
    │       ├── commands/
    │       │   ├── providers.ts
    │       │   ├── files.ts
    │       │   └── sync.ts
    │       └── api/
    │           └── client.ts
    │
    └── web/                  # @file-manager/web
        ├── package.json
        ├── tsconfig.json
        ├── vite.config.ts
        └── src/
            ├── main.tsx
            ├── components/
            │   ├── DualPane.tsx
            │   ├── FilePane.tsx
            │   ├── FileTree.tsx
            │   ├── FileList.tsx
            │   ├── ProviderSelector.tsx
            │   ├── SyncPanel.tsx
            │   └── ProgressOverlay.tsx
            ├── hooks/
            │   ├── useWebSocket.ts
            │   ├── useFiles.ts
            │   └── useSyncJobs.ts
            └── api/
                └── client.ts
```

### Server source layout (`packages/server/src/`)

| File | Responsibility |
|------|----------------|
| `index.ts` | Process entry point; reads config, starts Fastify |
| `server.ts` | Fastify instance creation, plugin registration, graceful shutdown |
| `provider-registry.ts` | Runtime mount/unmount of `StorageProvider` instances by `mountId` |
| `routes/files.ts` | File CRUD endpoints |
| `routes/providers.ts` | Provider mount/unmount endpoints |
| `routes/sync.ts` | Sync job CRUD + conflict resolution endpoints |
| `ws/events.ts` | WebSocket handler; fans out events to all connected clients |
| `sync/engine.ts` | Job registry; public API for creating, pausing, stopping jobs |
| `sync/job-runner.ts` | Per-job async loop; drives the full sync state machine |
| `sync/event-queue.ts` | In-process async queue; interface allows future Kafka adapter |
| `sync/conflict-resolver.ts` | Pure functions implementing each `ConflictStrategy` |
| `sync/snapshot-store.ts` | SQLite-backed (`better-sqlite3`) persistence of sync state |
| `sync/change-applier.ts` | Streaming read from source provider → write to dest provider |
| `providers/local.ts` | `LocalProvider` — reference `StorageProvider` implementation |

---

## 3. Tooling & Versions

| Tool | Choice | Reason |
|------|--------|--------|
| Package manager | **bun** | Fast installs, built-in workspace support, native test runner |
| Build orchestration | **turborepo** | Incremental, cached builds across packages |
| Language | **TypeScript 5.x** | Strict mode; the entire codebase is typed |
| API server | **Fastify** | First-class TypeScript, JSON schema validation, performant |
| CLI framework | **commander** | Type-safe, minimal, widely understood |
| Web framework | **React 19 + Vite** | Standard tooling; fast HMR; no custom bundler needed |
| Testing | **bun test** | Built-in; vitest-compatible API; zero extra dependency |
| Linting | **typescript-eslint (strict)** | Enforces strict patterns at lint time |
| Formatting | **prettier** | Consistent style; no debate |

---

## 4. TypeScript Configuration

### Root `tsconfig.base.json`

```jsonc
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022"],
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "useUnknownInCatchVariables": true,
    "noFallthroughCasesInSwitch": true,
    "verbatimModuleSyntax": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "esModuleInterop": false,
    "skipLibCheck": false
  }
}
```

### Per-package `tsconfig.json`

Each package extends the base and adds its own `include` and `outDir`:

```jsonc
{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "dist",
    "rootDir": "src"
  },
  "include": ["src"]
}
```

The `web` package additionally sets `"jsx": "react-jsx"`.

---

## 5. ESLint Configuration

Root `.eslintrc.cjs`:

```js
module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  parserOptions: { project: true },
  plugins: ['@typescript-eslint'],
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended-type-checked',
    'plugin:@typescript-eslint/strict-type-checked',
  ],
  rules: {
    '@typescript-eslint/no-floating-promises': 'error',
    '@typescript-eslint/await-thenable': 'error',
    '@typescript-eslint/no-misused-promises': 'error',
    '@typescript-eslint/prefer-nullish-coalescing': 'error',
    // Enforce async/await; ban .then()/.catch() chains
    '@typescript-eslint/no-misused-promises': ['error', {
      checksVoidReturn: true,
    }],
  },
};
```

`.then()` / `.catch()` chains are forbidden at lint time — use `async`/`await` exclusively.

---

## 6. Core Types (`@file-manager/shared`)

All types live in `packages/shared/src/index.ts` and are re-exported from the package root.

### File system types

```typescript
export interface FileEntry {
  name: string;
  path: string;
  type: 'file' | 'directory';
  size: number;        // bytes; 0 for directories
  modifiedAt: Date;
  mimeType?: string;
}

export interface FileStat extends FileEntry {
  createdAt: Date;
  etag?: string;       // for cache/conflict detection
  permissions?: string;
}

export type ChangeEventType = 'created' | 'modified' | 'deleted' | 'moved';

export interface ChangeEvent {
  type: ChangeEventType;
  path: string;
  destPath?: string;   // only for 'moved'
  timestamp: Date;
  providerMount: string;
}
```

### `StorageProvider` interface

```typescript
export interface StorageProvider {
  list(path: string): Promise<FileEntry[]>;
  read(path: string): AsyncIterable<Buffer>;
  write(path: string, data: AsyncIterable<Buffer>): Promise<void>;
  delete(path: string): Promise<void>;
  move(src: string, dest: string): Promise<void>;
  stat(path: string): Promise<FileStat>;
  watch(path: string): AsyncIterable<ChangeEvent>;
}
```

### Provider registry types

```typescript
export type ProviderScheme = 'local' | 's3' | 'gdrive' | 'sftp';

export interface ProviderMount {
  mountId: string;       // e.g. "documents", "backups"
  scheme: ProviderScheme;
  provider: StorageProvider;
  config: Record<string, unknown>;
}
```

### Sync engine types

```typescript
export type SyncDirection = 'one-way' | 'bidirectional';
export type ConflictStrategy = 'source-wins' | 'dest-wins' | 'newest-wins' | 'manual';
export type SyncJobStatus = 'running' | 'paused' | 'error' | 'stopped';

export interface SyncJob {
  id: string;
  source: { mount: string; path: string };
  dest:   { mount: string; path: string };
  direction: SyncDirection;
  conflictStrategy: ConflictStrategy;
  status: SyncJobStatus;
  createdAt: Date;
  lastSyncAt?: Date;
  errorMessage?: string;
}

export interface SyncSnapshot {
  jobId: string;
  path: string;
  etag: string;        // content hash or provider etag
  size: number;
  modifiedAt: Date;
  snapshotAt: Date;
}

export interface ConflictRecord {
  jobId: string;
  path: string;
  sourceEntry: FileStat;
  destEntry: FileStat;
  detectedAt: Date;
}
```

---

## 7. Provider URI Scheme

```
<scheme>://<mountId>/<path>
```

**Examples:**

```
local://documents/reports/q1.pdf
s3://backups/archive/2024/
gdrive://shared/team-docs/
sftp://prod-server/var/exports/
```

**Rules:**

- `scheme` identifies the provider type (`local`, `s3`, `gdrive`, `sftp`).
- `mountId` is the registered name in the server's provider registry.
- `path` is always POSIX-style (forward slashes); never URL-encoded in display or CLI output.
- All REST API path parameters that accept file locations use this URI format.

---

## 8. Storage Provider Backends

The initial implementation ships **local filesystem only**. The `StorageProvider` interface is the extension point — adding a new backend means:

1. Implementing `StorageProvider` in a new file under `packages/server/src/providers/`.
2. Registering it in `provider-registry.ts`.

No other code changes are required.

### `LocalProvider` (`packages/server/src/providers/local.ts`)

- `watch()` uses **chokidar** for reliable, cross-platform filesystem events.
- Configured with a whitelist of allowed root directories. Any path resolving outside the whitelist throws a `PermissionError` — there is no path traversal escape.
- `list()` returns entries sorted: **directories first**, then **files**, both groups alphabetical.

### Future provider constraints (design notes, not current scope)

| Provider | Watch mechanism | Notable constraint |
|----------|----------------|-------------------|
| S3 | S3 Event Notifications → SQS poll | No native watch; polling required |
| Google Drive | Drive API `changes.list` + `startPageToken` | Rate-limited polling |
| SFTP | Periodic `stat` polling | No native events |

These constraints inform how `watch()` must be designed for each provider but are not implemented in the initial release.

---

## 9. REST API Design

**Base URL:** `http://host:PORT/api/v1`

All error responses use the shape:
```json
{ "error": "Human-readable message", "code": "MACHINE_CODE" }
```
with an appropriate HTTP status code.

---

### Providers

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/providers` | List all mounted providers (returns `ProviderMount[]` without the `provider` instance) |
| `POST` | `/providers` | Mount a new provider. Body: `{ mountId: string, scheme: ProviderScheme, config: Record<string, unknown> }` |
| `DELETE` | `/providers/:mountId` | Unmount a provider |

---

### Files

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/files/:mountId/*path` | List directory → `FileEntry[]`, or redirect to file stream |
| `GET` | `/files/:mountId/*path?stat` | Return `FileStat` for any entry |
| `POST` | `/files/:mountId/*path` | Write file; body is raw stream; `Content-Length` required |
| `DELETE` | `/files/:mountId/*path` | Delete file or directory |
| `POST` | `/files/move` | Move or cross-provider copy. Body: `{ src: string, dest: string }` (provider URIs) |

---

### Sync Jobs

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sync/jobs` | List all sync jobs → `SyncJob[]` |
| `POST` | `/sync/jobs` | Create a job. Body: `Omit<SyncJob, 'id' \| 'status' \| 'createdAt'>` |
| `GET` | `/sync/jobs/:id` | Get a single job → `SyncJob` |
| `PATCH` | `/sync/jobs/:id` | Pause or resume. Body: `{ status: 'paused' \| 'running' }` |
| `DELETE` | `/sync/jobs/:id` | Stop and remove a job |
| `GET` | `/sync/jobs/:id/conflicts` | List unresolved conflicts → `ConflictRecord[]` |
| `POST` | `/sync/jobs/:id/conflicts/:path/resolve` | Resolve a conflict. Body: `{ resolution: 'use-source' \| 'use-dest' }` |

---

## 10. WebSocket Protocol

**Endpoint:** `ws://host:PORT/events`

- All messages are JSON.
- Communication is **server → client only**; clients do not send messages.
- Clients subscribe by connecting. They receive all events for all mounts.
- **Future:** add a `?mounts=documents,backups` filter parameter to the WS URL.

### Message envelope

```typescript
type WsMessageType =
  | 'file:change'    // ChangeEvent from any watched provider
  | 'sync:progress'  // incremental sync job activity (bytes transferred, files processed)
  | 'sync:conflict'  // new conflict detected requiring manual resolution
  | 'sync:status';   // SyncJob status changed (running → paused, error, stopped)

interface WsMessage<T> {
  type: WsMessageType;
  payload: T;
}
```

**Payload types by message type:**

| `type` | `payload` type |
|--------|---------------|
| `file:change` | `ChangeEvent` |
| `sync:progress` | `{ jobId: string; filesProcessed: number; bytesTransferred: number }` |
| `sync:conflict` | `ConflictRecord` |
| `sync:status` | `Pick<SyncJob, 'id' \| 'status' \| 'errorMessage'>` |

---

## 11. Sync Engine Design

The sync engine runs entirely inside the server process. Its source files live at `packages/server/src/sync/`.

### Component responsibilities

| File | Responsibility |
|------|----------------|
| `engine.ts` | Job registry; exposes `createJob`, `pauseJob`, `resumeJob`, `stopJob` |
| `job-runner.ts` | Per-job async loop; drives the state machine below |
| `event-queue.ts` | In-process async queue; see interface below |
| `conflict-resolver.ts` | Pure functions, one per `ConflictStrategy`; no side effects |
| `snapshot-store.ts` | SQLite (`better-sqlite3`) persistence of `SyncSnapshot` records |
| `change-applier.ts` | `read()` from source provider → `write()` to dest provider (streamed) |

### Bidirectional sync state machine

```
1. On job start:
   a. Take a full snapshot of both source and dest (list recursively, stat each file)
   b. Store snapshots in snapshot-store

2. Subscribe watch() on both source and dest (via job-runner)

3. On ChangeEvent from either side:
   a. Enqueue into event-queue

4. Consumer loop (job-runner):
   a. Dequeue next SyncQueueEntry
   b. Load last snapshot for that path from snapshot-store
   c. Determine if the change is new (i.e. not already reconciled)
   d. Check the other side's current stat vs. its snapshot
      → If other side also changed since last snapshot: CONFLICT
      → Otherwise: safe to apply
   e. If CONFLICT:
      - strategy = source-wins / dest-wins / newest-wins → resolve automatically
      - strategy = manual → emit sync:conflict WS message, skip path
   f. Apply change to other side via change-applier (streaming)
   g. Update snapshot for the path on both sides
   h. Emit sync:progress WS message

5. On pause: drain event-queue, suspend consumer loop
6. On resume: restart consumer loop
7. On error: set job status = error, emit sync:status WS message
```

### `EventQueue` interface

Keeping this interface clean allows swapping the in-process queue for Kafka without touching job-runner:

```typescript
export interface SyncQueueEntry {
  jobId: string;
  event: ChangeEvent;
  side: 'source' | 'dest';
}

export interface EventQueue {
  enqueue(entry: SyncQueueEntry): Promise<void>;
  consume(): AsyncIterable<SyncQueueEntry>;
}
```

The default implementation is an in-process async queue backed by an array + `Promise`-based signalling. A Kafka adapter would implement the same interface with a Kafka consumer group.

---

## 12. CLI Commands

**Binary name:** `files` (installed globally from `packages/cli`)

**Server URL:** read from `FILE_MANAGER_API_URL` environment variable; defaults to `http://localhost:4000`.

Large file transfers print progress via a spinner/progress bar (reads `Content-Length` from the response if available).

```
# Provider management
files providers list
files providers mount <mountId> --scheme <local|s3|gdrive|sftp> [--config key=value ...]
files providers unmount <mountId>

# File operations
files ls <uri>                         # list directory contents
files stat <uri>                       # show full file metadata
files cat <uri>                        # stream file contents to stdout
files cp <src-uri> <dest-uri>          # copy (streamed through server)
files mv <src-uri> <dest-uri>          # move / rename
files rm <uri>                         # delete file or directory

# Sync job management
files sync create \
  --src <uri> \
  --dest <uri> \
  [--direction one-way|bidirectional] \
  [--conflict source-wins|dest-wins|newest-wins|manual]

files sync list                        # list all sync jobs
files sync status <job-id>             # show job details
files sync pause <job-id>
files sync resume <job-id>
files sync rm <job-id>                 # stop and remove job

# Conflict resolution
files sync conflicts <job-id>          # list unresolved conflicts for a job
files sync resolve <job-id> <path> --use source|dest
```

All commands communicate exclusively with the REST API. The CLI has no direct access to storage providers.

---

## 13. Web Frontend Design

**Stack:** React 19, Vite, TypeScript, no external state management library.

State is managed with React built-ins (`useState`, `useReducer`, `useContext`). Redux or Zustand may be introduced if complexity demands it, but the default is to avoid it.

### Source layout (`packages/web/src/`)

```
components/
  DualPane.tsx          # Root layout: left pane + right pane side by side
  FilePane.tsx          # Single pane: ProviderSelector + FileTree + FileList
  FileTree.tsx          # Expandable directory tree sidebar
  FileList.tsx          # Main file listing with sort and filter controls
  ProviderSelector.tsx  # Dropdown to switch active provider/mount
  SyncPanel.tsx         # Sidebar: active sync jobs + conflict list
  ProgressOverlay.tsx   # Transfer progress display

hooks/
  useWebSocket.ts       # Connects to WS endpoint; dispatches incoming messages to local state
  useFiles.ts           # list / stat / read via REST API
  useSyncJobs.ts        # CRUD + real-time status for sync jobs

api/
  client.ts             # Typed fetch wrapper matching the REST spec (Section 9)
```

### Interaction model

- Each pane (`FilePane`) independently navigates a provider and path.
- **Drag-and-drop between panes:**
  - Same provider: fires `POST /files/move`.
  - Different providers: fires `POST /files/:mountId/*path` (streams the file through the server).
- Real-time updates: `useWebSocket` receives `file:change` events and triggers re-fetches via `useFiles`.
- Conflict badges appear in `SyncPanel` when `sync:conflict` WS messages arrive.

---

## 14. Testing Strategy

### Levels

| Level | Scope | Tool |
|-------|-------|------|
| Unit | Pure functions: `conflict-resolver`, `snapshot-store` comparisons, URI parsing | `bun test` |
| Integration | Each provider against a real or emulated backend (local tmp dirs, localstack for S3) | `bun test` |
| API | Fastify route testing via `fastify.inject()` — no network required | `bun test` |
| E2E | CLI commands against a live server instance with a local provider | `bun test` |

### Conventions

- Test files are **co-located** with source: `src/foo.ts` → `src/foo.test.ts`.
- Uses `bun test` (built-in, vitest-compatible API). No separate `vitest` dependency.
- Integration tests that require external services (S3, etc.) are gated behind an environment variable and skipped in CI unless the service is available.
- Each package has a `test` script in `package.json` that turborepo orchestrates via the pipeline.

---

## 15. Dependency Graph

```
@file-manager/shared        (no internal deps)
        ↑
@file-manager/server        (depends on shared; providers + sync engine live here)
        ↑               ↑
@file-manager/cli           @file-manager/web
(depends on shared only)    (depends on shared only)
(talks to server over HTTP) (talks to server over HTTP/WS)
```

The graph is acyclic. `cli` and `web` do **not** depend on `server` — they communicate exclusively over the network. This ensures:

- The CLI can run against a remote server without importing any server code.
- The web frontend is a pure browser bundle with no Node.js dependencies.
- `shared` remains importable by all packages without circular dependency risk.
