# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

The CLI has no default subcommand — every invocation names one (`generate` or
`models`). HuggingFace metadata fetched by `models search` is cached under
`models_dir/.hub-cache` and never expires on its own; `--refresh` re-fetches it.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.
