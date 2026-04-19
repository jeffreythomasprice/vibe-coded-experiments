# gcs CLI reference

Binary: `/home/jeff/workspaces/personal/gcs/gcs`

The binary is all flags, no subcommands. If any flag below seems wrong, re-run `gcs -help` — it's the authoritative source.

## Flag groups

### Validation

**`-validate [files...]`** — loads each file as the editor would. Prints `<path>\tok` on success. Non-zero exit if any file fails.

```
$ gcs -validate ~/.../Terra.gcs
/home/jeff/google_drive/games/campaign notes/space-game-2/Terra.gcs	ok
```

On failure, the line contains the error message instead of `ok`.

### Points report

**`-points [files...]`** — prints a JSON array; one object per input file.

```
$ gcs -points ~/.../Jeff-Sofec-the-Vulcan-CMO.gcs
[
  {
    "path": "/home/jeff/.../Jeff-Sofec-the-Vulcan-CMO.gcs",
    "total_points": 200,
    "spent": 200,
    "unspent": 0,
    "breakdown": {
      "ancestry": 0,
      "attributes": 28,
      "advantages": 107,
      "disadvantages": -35,
      "quirks": 0,
      "skills": 100,
      "spells": 0
    }
  }
]
```

Fields: `path` (string), `total_points` (int, the budget), `spent` (int, sum of everything), `unspent` (int, `total_points - spent`), `breakdown` (per-category int subtotals — `disadvantages` and `quirks` are negative, advantages/skills/spells/attributes/ancestry are non-negative).

### Library search

**`-search <substring>`** — case-insensitive substring match against name, local notes, user description, markdown, or tags of every library item.

**`-search-type <csv>`** — restrict by type. Valid values:
- `traits`
- `trait-modifiers`
- `skills`
- `spells`
- `equipment`
- `equipment-modifiers`
- `notes`
- `templates`

Defaults to all types. Unknown value produces `unknown --search-type value "X" (valid: [list])`.

**`-search-out <file>`** — write the JSON results to a file instead of stdout.

**Result envelope** — always a JSON array; each element:

```json
{
  "type": "skill",
  "source_file": "/home/jeff/GCS/Master Library/Basic Set/Basic Set Skills.skl",
  "container": "Optional parent container name",
  "data": { ... type-specific fields ... }
}
```

`container` is only present when the item lives inside a container. `data` is the full library record — its shape depends on `type` (see `references/json-format.md` for each type's fields).

**Example — skill search:**

```
$ gcs -search scrounging -search-type=skills
[
  {
    "type": "skill",
    "source_file": "/home/jeff/GCS/Master Library/Basic Set/Basic Set Skills.skl",
    "data": {
      "id": "sQ2Nj_TqMYKV7skUF",
      "name": "Scrounging",
      "reference": "B218",
      "tags": ["Criminal", "Street"],
      "difficulty": "per/e",
      "defaults": [{"type": "per", "modifier": -4}],
      "points": 1
    }
  },
  ...
]
```

Many entries come back from different sourcebooks. Prefer the one from `Basic Set/` unless the user asked for a specific book.

### Export

**`-pdf [files...]`** — exports each `.gcs` to PDF. **Requires a display server.** On this headless host, wrap with xvfb:

```
xvfb-run /home/jeff/workspaces/personal/gcs/gcs -pdf path/to/char.gcs
```

**`-pdf-out <dir>`** — output directory for PDFs (defaults to current working directory).

**`-text <template-file>`** — export using a text template. Emits `<character-name>.txt` next to the input file. Silent on success.

### Maintenance

**`-convert [paths...]`** — upgrades files to the current data format. Accepts directories (recursive). Exits after processing. Use on older v2 / v4 sheets to bump to v5.

**`-sync [paths...]`** — syncs `.gcs` and `.gct` files with their library sources (re-links items to current library IDs / refreshes library-derived fields). Recursive for directories. Exits after processing.

### Logging / settings / meta

- `-console` — also copy logs to stdout/stderr
- `-log-file <file>` — log destination (default `~/.local/share/com.trollworks.gcs/Logs/gcs.log`)
- `-log-file-backups <n>` — rotated log count (default 1)
- `-log-file-size <bytes>` — max log size before rotating (default 10 MiB)
- `-log-level DEBUG|INFO|WARN|ERROR`
- `-settings <file>` — alternative prefs file (default `~/.local/share/com.trollworks.gcs/gcs_prefs.json`)
- `-v` — short version
- `-version` — full version

## Exit codes

- `0` — success
- non-zero — any validation failure, unknown flag, or processing error

## Gotchas

- File paths with spaces — always quote them. The campaign notes dir has spaces in names.
- `-search` only hits the configured master library (`~/GCS/Master Library/` by default); it does NOT search character sheets. To inspect what's in a specific sheet, parse the JSON directly.
- `-pdf` without `xvfb-run` on this box will error trying to open a display.
- `-convert` modifies files in place. Back up first if the user cares.
