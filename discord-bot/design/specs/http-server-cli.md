# HTTP Server & CLI for Discord Bot

**Summary:** Refactor the bot entry point into a server with an HTTP API for sending messages, add Zod-validated config and request schemas, and introduce a CLI client that reads config and sends messages via the API.
**Depends on:** None

---

## Steps

### 1.1 Install dependencies and rename entry point

**Files:** `package.json`, `src/server.ts` (renamed from `src/index.ts`)

- `bun add zod`
- Rename `src/index.ts` to `src/server.ts`
- Update `package.json` scripts: `"start": "bun run src/server.ts"`, `"dev": "bun --watch src/server.ts"`
- Update `"module"` field to `"src/server.ts"`
- Verify the bot still starts with `bun run dev`

### 1.2 Add config schema and loader

**Files:** `src/config.ts`

- Define a Zod schema `configSchema` with:
  - `host`: `z.string().default("127.0.0.1")`
  - `port`: `z.number().int().default(45192)`
  - `defaultRecipient`: `z.string().default("jeff0587")`
- Export `type Config = z.infer<typeof configSchema>`
- Export `function loadConfig(path?: string): Config` that:
  - Defaults path to `./discord-bot.config.json` (relative to cwd)
  - Reads the file with `Bun.file(path).json()`
  - Parses and validates with `configSchema.parse()`
  - Throws a clear error if the file doesn't exist or validation fails
- Create the default config file `discord-bot.config.json` in the project root:
  ```json
  {
    "host": "127.0.0.1",
    "port": 45192,
    "defaultRecipient": "jeff0587"
  }
  ```

### 1.3 Add message request schema

**Files:** `src/schemas.ts`

- Define `messageRequestSchema = z.object({ message: z.string().min(1) })`
- Export `type MessageRequest = z.infer<typeof messageRequestSchema>`

### 1.4 Add HTTP server to server.ts

**Files:** `src/server.ts`

- Import `loadConfig` from `./config` and `messageRequestSchema` from `./schemas`
- After the Discord client is ready, start a `Bun.serve()` HTTP server using config `host` and `port`
- Route `POST /message`:
  - Parse JSON body, validate with `messageRequestSchema.safeParse()`
  - On validation failure: return `400` with `{ error: string }` containing the Zod error message
  - On success: look up the `defaultRecipient` user across all guilds (use `client.users.cache.find(u => u.username === config.defaultRecipient)` or fetch), then send a DM or channel message @mentioning that user
  - Return `200` with `{ success: true }`
  - If user not found: return `404` with `{ error: "Recipient not found" }`
- All other routes: return `404 { error: "Not found" }`
- Log the server URL on startup: `Server listening on http://${config.host}:${config.port}`

### 1.5 Add CLI app

**Files:** `src/cli.ts`

- Add a new script in `package.json`: `"cli": "bun run src/cli.ts"`
- Use Bun's `process.argv` to parse a simple subcommand structure (no external CLI framework needed):
  - `bun run cli message "hello world"` — sends POST /message with `{ message: "hello world" }`
- Load config using `loadConfig()` to get host and port
- Build the URL: `http://${config.host}:${config.port}/message`
- Send the request with `fetch()`, print the response JSON to stdout
- On error (non-2xx or network failure), print the error and `process.exit(1)`

### 1.6 Update CLAUDE.md and README

**Files:** `CLAUDE.md`, `README.md`

- Update `CLAUDE.md` architecture section to reflect:
  - `src/server.ts` — Discord client + HTTP server (replaces index.ts)
  - `src/config.ts` — Config schema and loader
  - `src/schemas.ts` — Zod request validation schemas
  - `src/cli.ts` — CLI client for the HTTP API
  - `src/events/` — Event handlers (unchanged)
- Update commands section to include `bun run cli message "text"`
- Create or update `README.md` with:
  - Project description
  - Setup: `bun install`, create `.env` with `DISCORD_TOKEN`
  - Running the server: `bun run dev` or `bun run start`
  - Config file: explain `discord-bot.config.json` and its properties
  - Using the CLI: `bun run cli message "your message"`
