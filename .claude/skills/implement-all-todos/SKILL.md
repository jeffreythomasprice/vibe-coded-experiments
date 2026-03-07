---
name: implement-all-todos
description: Implement all remaining TODO items from a todo list file, each in a separate subagent
disable-model-invocation: true
argument-hint: "<spec-name> [optional: phase number, e.g. 2]"
---

# Implement All Remaining TODO Items

Parse `$ARGUMENTS` to extract the spec name and optional phase filter. The first argument is the spec name (e.g. `memory-subsystem`), any remaining arguments are an optional phase number filter. The todo file is at `design/specs/<spec-name>-todos.md` and the detailed spec is at `design/specs/<spec-name>.md`.

Example: `/implement-all-todos memory-subsystem` or `/implement-all-todos projectile-physics 2`

Implement every unchecked TODO item from the todos file, running each in a separate subagent for parallelism where possible.

## Workflow

1. **Read the todo file** at `design/specs/<spec-name>-todos.md` and collect all unchecked items (`- [ ]`).
2. If a phase number is provided in `$ARGUMENTS` (e.g. `2`), filter to only items in that phase. Otherwise process all unchecked items.
3. **Group items by phase**. Items within a phase may have dependencies on earlier items in the same phase (e.g. 2.1 defines types used by 2.5), so implement each phase sequentially by item number.
4. **For each unchecked item**, launch an Agent subagent (subagent_type: "general-purpose") with a prompt that includes:
   - The item number and description
   - Instructions to follow the `/implement-todo` workflow:
     a. Read the todo file (`design/specs/<spec-name>-todos.md`) to confirm the item is not checked off
     b. Read the spec file (`design/specs/<spec-name>.md`) for the detailed spec
     c. Read existing code in the relevant module for conventions
     d. Implement the item following the spec
     e. Write tests in a `#[cfg(test)] mod tests` block
     f. Register the module in the parent `mod.rs` if it's a new file
     g. Run `cargo test` scoped to the new module
     h. Update the todo file to check off the item
   - The full path to the project: `/home/jeff/workspaces/personal/vibe-coded-experiments/robowar`
   - Remind the agent to follow `CLAUDE.md` conventions (anyhow, u32 bit-casting, minimal comments)
5. **Within a phase**, run items that have no dependencies on each other in parallel. Items that clearly depend on earlier items (check the implementation plan) should wait. When unsure, run sequentially.
6. **After each phase completes**, run `cargo test` and `cargo clippy` in the main context to verify everything integrates correctly before moving to the next phase.
7. **After all items are done**, run a final `cargo test` and `cargo clippy` to confirm the full build passes, then report a summary of what was implemented.
