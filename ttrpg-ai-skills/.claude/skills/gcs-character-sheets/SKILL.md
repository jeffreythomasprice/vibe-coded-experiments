---
name: gcs-character-sheets
description: Use this skill when the user is creating, editing, validating, inspecting, searching, or exporting GURPS character sheets (GCS files, .gcs, .gct). Triggers on phrases like "create a GURPS character", "build a GCS sheet", "validate my character", "check points", "search the GURPS library", "export to PDF", or any mention of a `.gcs` file path (often under `~/google_drive/games/campaign notes/`). GCS = GURPS Character Sheet. Use this even when the user doesn't say "GURPS" explicitly if the context involves ST/DX/IQ/HT, CP totals, or the gcs binary.
version: 1.0.0
---

# GCS (GURPS Character Sheet) Skill

You are helping the user work with GCS — the GURPS Character Sheet editor by Richard Wilkes. `.gcs` files are JSON; the `gcs` binary is a CLI that can validate, report points, search the library, and export.

## Binary

```
/home/jeff/workspaces/personal/gcs/gcs
```

Always use this absolute path. The binary takes flags only, no subcommands. `gcs -help` is authoritative if something in these docs seems off.

## Workflow map

Read the reference for whichever workflow the user is asking about — don't preload them.

| Task | Reference |
|---|---|
| Any CLI invocation, output shape, or flag question | `references/cli.md` |
| JSON structure of a `.gcs` file — traits, skills, equipment, etc. | `references/json-format.md` |
| Building a new character from scratch | `references/creating-characters.md` |
| Attribute costs, body locations, damage table, common advantages/disadvantages, tech levels | `references/gurps-reference.md` |

## Rules to follow every time

1. **Search the library before writing trait/skill/equipment JSON by hand.** Use `gcs -search <term> -search-type=<type>`. Copy the resulting `id`, `name`, `reference`, `difficulty`, `defaults`, `tags` into the sheet. Fabricating these fields produces items that don't match library entries.

2. **Prefer Basic Set results.** When multiple `-search` results come back, pick the one whose `source_file` contains `Basic Set` (or whose `reference` starts with `B`) unless the user has specified a different sourcebook. Other libraries seen: Action, Dungeon Fantasy RPG, Discworld, After the End, etc.

3. **Always validate after editing.** Run `gcs -validate <file>` after any JSON change. Then `gcs -points <file>` to confirm the point total matches intent.

4. **Headless PDF export.** This machine has no display server for the GCS UI. Use:
   ```
   xvfb-run /home/jeff/workspaces/personal/gcs/gcs -pdf <file>
   ```

5. **Version 5 is current.** New sheets you create should be `"version": 5`. Older sheets (v2, v4) on disk still work — `gcs -convert <file>` upgrades them in place.

6. **Don't open the reference sheets under `~/google_drive/games/campaign notes/` unless the user explicitly asks you to.** Everything needed to build a new sheet is in `references/gurps-reference.md` and `references/json-format.md`.

## Common one-liners

```bash
# Validate
/home/jeff/workspaces/personal/gcs/gcs -validate path/to/char.gcs

# Points report (JSON)
/home/jeff/workspaces/personal/gcs/gcs -points path/to/char.gcs

# Library search (prefer Basic Set results)
/home/jeff/workspaces/personal/gcs/gcs -search "combat reflexes" -search-type=traits
/home/jeff/workspaces/personal/gcs/gcs -search "stealth" -search-type=skills
/home/jeff/workspaces/personal/gcs/gcs -search "kevlar" -search-type=equipment

# PDF export (headless)
xvfb-run /home/jeff/workspaces/personal/gcs/gcs -pdf path/to/char.gcs
```

## When the user asks to "make a character"

Don't start writing JSON immediately. Ask briefly (one message, batched):

1. Point total (100/150/200/250)?
2. Tech level (TL3 medieval … TL8 near-future … TL10+ sci-fi)?
3. Campaign flavor / concept in one sentence?
4. Where should the file be saved?

Then follow `references/creating-characters.md`.
