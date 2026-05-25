# ecs — Exalted Character Sheet

A Rust crate for creating and managing Exalted (2nd edition) Solar character
sheets. Ships as a library and a CLI binary named `ecs` (short for **Exalted
Character Sheet**). With no subcommand, `ecs` launches an egui-based desktop
editor; subcommands cover validation, rendering, and queries against the
embedded rules database.

## Build / install

```
cargo build --release
# binary: target/release/ecs
```

Put `target/release/ecs` on your `PATH` (or `cargo install --path .`) so the
example skill below can find it.

## A few common invocations

```
ecs                                          # GUI editor, blank
ecs assets/sample-character.toml             # GUI editor, file pre-opened
ecs render assets/sample-character.toml      # markdown to stdout
ecs render --format pdf assets/sample-character.toml -o /tmp/sheet.pdf
ecs validate assets/sample-character.toml
ecs backgrounds                              # list every Background
ecs charms first-archery-excellency          # show one Charm
```

Run `ecs --help` for the full subcommand list.

## Example skill

`example-skills/create-character/SKILL.md` is an **example** Claude Code skill
that walks a user through building a Solar character TOML interactively, then
validates it with `ecs`. It is not installed automatically — copy it into your
own `.claude/skills/create-character/` (or another skill directory Claude Code
loads) if you want to use it.

The skill assumes `ecs` is on `PATH`; it doesn't try to build or locate the
binary itself. If `ecs --help` fails, install the binary first.
