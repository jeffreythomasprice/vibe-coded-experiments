# Headless server (`editor-server`)

One process owns every open document. The GUI, the CLI and the MCP server are clients of it over a unix socket, so an edit by any of them reaches all the others: an agent can build while a human watches in the GUI. Companion docs: [plan](plan.md), [commands](tier0-commands.md), [MCP](tier0-mcp.md), [GUI](gui.md).

```
editor-mcp (stdio shim) ─┐
editor-cli ──────────────┼─ unix socket, JSON lines ─> editor-server ─> editor_host::Hub ─> Sessions
editor-app (replica) ───┤   (editor-proto)                                 └─ one lazy Renderer
editor-game (replica) ──┘
```

## Crates

- `editor-proto` (GPU-free): the wire types, `Config`, `Policy`, and a blocking `Client` with autospawn. Every client depends on this and nothing heavier.
- `editor-host`: `Hub`, the state machine. It holds the sessions, the connections (each with a client name, a `Policy` and a working directory), the focused session and the one shared renderer. It does no I/O: `Hub::handle(conn, request)` returns the reply plus the events to deliver, which makes it unit-testable. Only the server depends on it.
- `editor-server`: the binary. A thread reads each connection, a second thread writes to it, and one mutex around the `Hub` makes every edit single-writer. Events are queued before the reply on the same connection, so a client that has its reply has already received the events that caused it.
- `editor-proto::Replica`: a local copy of one session (document, selection, face selection, revision, file and history info) that applies `ServerEvent`s (snapshots by replacement, `changed` by insert and remove). `editor-app` and `editor-game` both build on it; it needs no GPU.
- `editor-game`: a client like the GUI, with the client name `game` and no mutate access. Only a `gui` client moves the focused session, so the game never steals it. See [game.md](game.md).
- `editor-cmd` knows nothing of any of this beyond the `Actor`, the change feed and the `Exec::Server` ops below.

## Config

One TOML file shared by all processes, so they agree where the socket is. Looked up at `--config`, else `$EDITOR_CONFIG`, else `$XDG_CONFIG_HOME/editor/config.toml` (`~/.config/...`). A missing default file means defaults; unknown keys are errors.

```toml
socket = "/run/user/1000/editor.sock"   # default $XDG_RUNTIME_DIR/editor.sock, else /tmp/editor-<uid>.sock
[server]
idle_exit_secs = 5       # exit after this long with no clients and no unsaved session
log = "..."              # default <socket>.log
binary = "..."           # editor-server to autospawn; default next to the running executable, else PATH
[mcp]
allow_mutate = false     # defaults for editor-mcp; its flags add to them
allow_destructive = false
```

The socket path must be at most 100 bytes (the unix limit is about 108); a longer one is a config error. The server takes an exclusive lock on `<socket>.lock`, so two racing autospawns leave exactly one server, and it creates the socket with mode 0600.

## Lifecycle

Any client autospawns the server when it cannot connect (detached into its own process group, output to the log, up to 3 s to come up). The server exits after `idle_exit_secs` with no connections **and** no dirty session. A session with unsaved changes and no clients stays in memory, and the server with it, so a later client can attach and save. `editor-cli server stop` refuses while a session is dirty unless `--force`.

## Protocol

One JSON object per line each way, version `PROTOCOL = 2`. Every message is a typed Rust enum or struct in `editor-proto`'s `wire.rs` with a derived JSON Schema that rejects unknown fields; `editor-cli schema --wire` prints them, and `crates/editor-proto/tests/wire.rs` checks samples against them. A client must send `hello` first; a protocol mismatch is answered with a `Protocol` error and the connection closes (restart the server).

Client to server:
- `{"type":"hello","id":0,"protocol":2,"client":"gui","mutate":true,"destructive":true,"cwd":"/path"}`. `client` is the attribution name (`gui`, `cli`, `mcp`, `game`). The two flags are this connection's `Policy`. Relative `path` arguments of `session.attach`, `file.open`, `file.new`, `file.save` and `view.screenshot` resolve against `cwd`.
- `{"type":"request","id":N,"request":{op, args, dry_run?, expect_revision?, merge_key?}}`, the [envelope](tier0-commands.md#envelope).
- `{"type":"control","id":N,"action":"status"|"stop","force":false}`: server administration, not part of the op registry (`ControlAction`; any other action is rejected when the message is parsed). `stop` needs a destructive connection. `status` replies with `ControlStatus {protocol, connections, dirty_sessions, sessions}`, `stop` with `StopAck {stopping}`.

Server to client:
- `{"type":"reply","id":N,"response":{...},"png_base64"?}`. `response` is the typed [envelope](tier0-commands.md#envelope) (`Success` or `ErrorResponse`); the shape of a success `result` is the op's `result_schema`. The hello reply carries `HelloResult {conn, protocol}`.
- `{"type":"snapshot","session","revision","document_toml","selection","face_selection","file":{path,name,dirty},"history":{can_undo,can_redo,transaction:{holder,conn,label}|null}}`: the whole document in the [scene format](scene-format.md). Sent on attach, and again when `file.open` / `file.new` replace the document.
- `{"type":"changed","session","revision","upsert":[Object],"deleted":[id],"selection"?,"face_selection"?,"file"?,"history"?}`: the objects whose content changed (full JSON, as in `scene.get`), the ids that are gone, and whichever of selection, file state and history state changed. It covers every cause alike: a commit, an edit inside an open transaction, a rollback, undo, redo.

Events go to every connection attached to the session, the sender included. A client holding a replica applies snapshots by replacement and `changed` by insert and remove; `revision` is the session revision after the change.

## Sessions

A session is a document, its file path, selection and face selection, undo history and revision. The hub keys them `s1`, `s2`...

- A connection starts with no session; every op other than `session.*` fails until it attaches.
- `session.attach {id | path, create?}`: by `path` it joins the session that has that file open (paths are canonicalized), else loads the file; `create` writes a new scene there when it is missing (a missing file without `create` is an `Io` error). With neither argument it joins the **focused** session, creating an in-memory `untitled` one if there is none. The result is the session row plus `created: true` when this call made the session. Attaching again switches sessions; the connection's open transaction in the old one is rolled back.
- The focused session is the one a `gui` client attached to most recently, else the first one created. This is what makes `editor-mcp` with no file join whatever the GUI has open.
- `session.list` (sessions with their clients and the focused flag) and `session.info` (this connection's session).
- A file can be open in one session only. `file.open`, `file.new {path}` and `file.save {path}` onto a path another session owns fail with `FileInUse`; join that session instead.
- A clean session with no clients is dropped. A dirty one is kept (see Lifecycle).

## Concurrency

- **Single writer.** All ops run under the hub mutex, one at a time; `expect_revision` rejects a write made against a stale view.
- **Transactions belong to a connection.** While one connection has `history.begin` open, every other connection's mutating op (document ops, `history.*` other than `list`, `file.*` other than `info`) fails with `SessionBusy {holder, label}`. Reads and `select.*` still work. If the owner disconnects, its transaction is rolled back and the rollback is broadcast. Keep transactions short when someone may be watching.
- **Merging.** A request with `merge_key` folds into the previous undo entry when that entry is the top of the undo stack and has the same connection and key (see [semantics](tier0-commands.md#semantics)). It holds no lock, so other clients keep working during a drag.
- **Undo is shared.** `history.undo` pops whatever entry is on top of the session, whoever made it; `history.list` shows who.

## Policy

Each connection has its own `Policy` (read, mutate, destructive), checked by the hub before routing, `batch` contents included. `editor-mcp` takes it from its flags and the config; the CLI and GUI ask for everything, and `editor-cli session --read-only` asks for nothing but reads.

## Not yet

Authentication beyond the socket's file permissions, a revision-gap resync in the replica protocol (ordering on one socket makes gaps impossible today), pushing events to the MCP agent as notifications, and a TCP transport for remote editing.
