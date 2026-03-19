# file-sync

A CLI tool that synchronizes files and directories across multiple locations. Useful for keeping dotfiles, config files, or any other files in sync between different paths (e.g., your home directory and a cloud storage folder).

## Building

Requires Rust (edition 2024).

```sh
cargo build --release
```

The binary will be at `target/release/file-sync`.

## Usage

```
file-sync [--config <path>] [--log-dir <path>] [--entity <name>...] [--dry-run]
```

| Flag | Description |
|------|-------------|
| `--config <path>` | Path to config file |
| `--log-dir <path>` | Directory for log files (default: `/tmp/file-sync/logs`) |
| `--entity <name>` | Only sync specific entities by name (repeatable) |
| `--dry-run` | Show what would happen without making changes |

### Config file resolution

The config file is found in this order:

1. `--config` flag
2. `$FILE_SYNC_CONFIG` environment variable
3. `~/.config/file-sync/config.toml`

## Config file

The config is a TOML file listing sync entities. Each entity has a name and a list of paths that should be kept in sync.

```toml
# Optional: override the default log directory
log_dir = "/home/me/.local/share/file-sync/logs"

# Each [[sync]] block defines a set of paths to keep in sync.
# Paths support tilde expansion and globs.

[[sync]]
name = "bashrc"
paths = [
    "~/dotfiles/.bashrc",
    "~/.bashrc",
]

[[sync]]
name = "nvim-config"
paths = [
    "~/dotfiles/nvim",
    "~/.config/nvim",
]

[[sync]]
name = "ssh-config"
paths = [
    "~/dotfiles/.ssh/config",
    "~/Dropbox/backup/.ssh/config",
]
```

## How it works

For each sync entity, file-sync compares all listed paths:

- **No copies exist** — skipped.
- **One copy exists** — copied to all missing locations.
- **Multiple identical copies** — copied to any missing locations.
- **Multiple differing copies** — opens a TUI side-by-side diff viewer where you choose which version to keep.

Files are compared by full byte content. The TUI diff viewer supports vim-style keybindings.
