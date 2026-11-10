# Tier 0 — MCP server and session host

`level-mcp` exposes the [command catalog](tier0-commands.md) as MCP tools over stdio. It shares a session host with the CLI's `session` mode, so both behave the same. Companion docs: [plan](plan.md), [model](tier0-model.md).

## Crates

- `level-host`: `Host` owns a `level_cmd::Session` and a lazily created, long-lived `level_render::Renderer`. `Host::execute` applies the permission `Policy`, routes `Exec::Client` ops (`view.screenshot`) to `level_render::screenshot_with_selection`, and sends everything else to `Session::execute`. It returns the response plus the PNG bytes, if any.
- `level-mcp`: the stdio server. It holds `Arc<Mutex<Host>>` and runs each call on a blocking thread, since rendering blocks. It is one writer: calls are serialized.
- `level-cli session` uses the same `Host`.

## Running

```
level-mcp [FILE] [--create] [--allow-mutate] [--allow-destructive]
```

- With `FILE` the document is loaded and `file.save` writes back to it. `--create` writes a new level there if the file is missing; without it a missing file is a startup error.
- Without `FILE` the document lives in memory (`file.info` reports a null `path`) and the agent binds a file at runtime with `file.save {path}` or `file.open`. Registering without `FILE` is the recommended setup.
- Nothing reaches disk until `file.save`. Exiting with unsaved changes prints a warning on stderr (stdout carries the protocol; the server never prints there).
- Register it with Claude Code: `claude mcp add editor -- /path/to/level-mcp my.level.toml --allow-mutate`.

## Gating

Every op has a safety class (`read`, `mutate`, `destructive`). The server starts read-only:

| Flag | Enables |
|---|---|
| (none) | `read` ops |
| `--allow-mutate` | `read` + `mutate` |
| `--allow-destructive` | everything (implies `--allow-mutate`) |

Tools the policy does not allow are not listed. A call to one anyway, or a `batch` containing one, fails with `NotPermitted` (the check walks into `batch.commands`). `select.*`, `file.save` and the `history.*` ops that change state are `mutate`; `history.undo`, `file.new`, `file.open`, `object.delete`, `layer.delete` and `view.camera.delete` are `destructive`.

## Tools

One tool per registry op, generated from `OPS`:

- **Name:** the op name with `.` replaced by `_` (`brush.create_box` → `brush_create_box`; `batch` stays `batch`), since tool names may not contain dots.
- **Description:** the op's one-line `summary`.
- **Input schema:** the op's args schema with `$ref`s inlined and `type: "object"` at the root. Non-read ops also get `expect_revision`, and `Document` ops `dry_run`; the server moves both into the command envelope. Everything else is the command's `args`.
- **Annotations:** `readOnlyHint` for `read` ops, `destructiveHint` for `destructive` ones, `openWorldHint: false`.
- **Result:** a text block with the compact response JSON (`{ok, revision, result, changes, warnings}` or the structured error). `isError` is set when `ok` is false. Errors are tool results, not protocol errors, so the agent sees `code`, `hint` and `field`. An unknown tool name is a protocol error.
- **`view_screenshot`:** the same text block, then an `image/png` content block.

Results have no output schema yet; see the command docs.

## Instructions

The server's `instructions` carry the workflow: `scene_summary` first; `history_begin` (or `batch`) for multi-step edits; `check_validate`; `view_screenshot` from a few cameras with the `labels` overlay to map pixels to ids; adjust; `history_commit` or `history_rollback`; `file_save` to persist.

## Attribution

Transactions record the client name `mcp` (`cli` for the CLI), shown by `history.list`.

## Not yet

A GUI editor sharing the same session (the host is the seam: the GUI would be another client of one `Host`), change events pushed to clients, and `outputSchema`.
