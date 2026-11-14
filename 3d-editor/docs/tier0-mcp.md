# Tier 0 — MCP server and session host

`editor-mcp` exposes the [command catalog](tier0-commands.md) as MCP tools over stdio, as a client of the [headless server](server.md). Companion docs: [server](server.md), [plan](plan.md), [model](tier0-model.md).

## Crates

`editor-mcp` is a thin stdio client of [`editor-server`](server.md), which owns the session. It holds one `editor_proto::Client` (autospawning the server if needed) and forwards each tool call as a request; `view.screenshot` renders in the server and the PNG comes back over the socket. Tool definitions are generated locally from `OPS`, so the shim never loads the renderer. The CLI's `session` mode and the GUI are clients of the same server, so all three see one document and each other's edits.

## Running

```
editor-mcp [FILE] [--create] [--allow-mutate] [--allow-destructive] [--config PATH]
```

- With `FILE` the shim attaches to the session for that file (loading it if no client has it open) and `file.save` writes back to it. `--create` writes a new scene there if the file is missing (it needs `--allow-mutate`); without it a missing file is a startup error.
- Without `FILE` the shim attaches to the server's focused session: the GUI's document when one is open (the live-view workflow), else a new in-memory one (`file.info` reports a null `path`) that the agent binds with `file.save {path}`, or switches with `session_attach`. Registering without `FILE` is the recommended setup.
- Nothing reaches disk until `file.save`. Unsaved changes stay in the server after the agent disconnects; exiting with some prints a warning on stderr (stdout carries the protocol; the shim never prints there).
- Flags add to the `[mcp]` defaults in the shared config (`allow_mutate`, `allow_destructive`).
- Register it with Claude Code: `claude mcp add editor -- /path/to/editor-mcp --allow-mutate`.

## Gating

Every op has a safety class (`read`, `mutate`, `destructive`). The server starts read-only:

| Flag | Enables |
|---|---|
| (none) | `read` ops |
| `--allow-mutate` | `read` + `mutate` |
| `--allow-destructive` | everything (implies `--allow-mutate`) |

Some arguments raise the class: `session.attach` with `create: true` needs mutate (it writes a file), `csg.subtract` with `consume_cutters: true` needs destructive (it deletes objects), and `view.screenshot` with `path` needs mutate (it writes a file). Tools the policy does not allow are not listed; the policy is also the connection's policy at the server, which enforces it. A call to one anyway, or a `batch` containing one, fails with `NotPermitted` (the check walks into `batch.commands`). `select.*`, `file.save` and the `history.*` ops that change state are `mutate`; `history.undo`, `file.new`, `file.open`, `object.delete`, `layer.delete` and `view.camera.delete` are `destructive`.

## Tools

One tool per registry op, generated from `OPS` (including `session_list`, `session_attach` and `session_info`):

- **Name:** the op name with `.` replaced by `_` (`brush.create_box` → `brush_create_box`; `batch` stays `batch`), since tool names may not contain dots.
- **Description:** the op's one-line `summary`.
- **Input schema:** the op's args schema with `$ref`s inlined and `type: "object"` at the root. Non-read ops also get `expect_revision`, and `Document` ops `dry_run`; the server moves both into the command envelope. Everything else is the command's `args`.
- **Annotations:** `readOnlyHint` for `read` ops, `destructiveHint` for `destructive` ones, `openWorldHint: false`.
- **Output schema:** `anyOf` the op's `response_schema` (`{ok, revision, result, changes, warnings}` with that op's typed `result`) and the structured error, `$ref`s inlined, `type: "object"` at the root.
- **Result:** `structuredContent` with the response JSON, and the same JSON as a text block for clients that only read text. `isError` is set when `ok` is false. Errors are tool results, not protocol errors, so the agent sees `code`, `hint` and `field`. An unknown tool name is a protocol error.
- **`view_screenshot`:** the same text block, then an `image/png` content block. With `path` the server also writes the PNG there; writing a file is a side effect, so that call needs `--allow-mutate` (a read-only server still renders without `path`).

## Instructions

The server's `instructions` carry the workflow: `scene_summary` first; `history_begin` (or `batch`) for multi-step edits; `check_validate`; `view_screenshot` from a few cameras with the `labels` overlay to map pixels to ids; adjust; `history_commit` or `history_rollback`; `file_save` to persist; the clipboard (`clipboard_copy` / `clipboard_paste`, shared with the GUI and across files) and recovery of autosaves after a crash (`recovery_list`, `recovery_restore`); and the `batch` item shape (`{op, args, as?}`, dotted or underscored names, `$name` references).

## Attribution

Transactions record the client name `mcp` (`cli` for the CLI, `gui` for the editor), shown by `history.list`. While the agent holds a transaction, other clients' edits fail with `SessionBusy`, so the instructions tell it to keep transactions short.

## Not yet

Pushing change events to the agent as MCP notifications (the shim discards them).
