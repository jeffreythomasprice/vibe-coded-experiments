---
name: implement-todo
description: Implement a TODO item from a todo list file following the implementation plan
disable-model-invocation: true
argument-hint: "<spec-name> <item number, e.g. 1.5>"
---

# Implement a TODO Item

Parse `$ARGUMENTS` to extract the spec name and item number. The first argument is the spec name (e.g. `memory-subsystem`), the remaining arguments are the item identifier. The todo file is at `design/specs/<spec-name>-todos.md` and the detailed spec is at `design/specs/<spec-name>.md`.

Example: `/implement-todo memory-subsystem 1.5` or `/implement-todo projectile-physics 3.2`

## Workflow

1. **Read the todo file** at `design/specs/<spec-name>-todos.md` to find the item and confirm it is not already checked off.
2. **Read the spec file** at `design/specs/<spec-name>.md` to get the detailed spec for that item (struct definitions, methods, behavior, test expectations).
3. **Read existing code** in the relevant module to understand conventions, imports, and how neighboring files are structured.
4. **Implement** the item following the spec. Match the conventions already established in the codebase (error handling with `anyhow`, `u32` bit-casting pattern, etc.).
5. **Write tests** inline in a `#[cfg(test)] mod tests` block covering the cases listed in the spec plus edge cases.
6. **Register the module** in the parent `mod.rs` if it's a new file.
7. **Run `cargo test`** scoped to the new module to verify everything passes.
8. **Update the todo file** to check off the completed item (`- [ ]` → `- [x]`).
9. **Update CLAUDE.md** — Review the Architecture section and update it to reflect the current project structure. Add new modules/files you created, move items from "Planned but not yet implemented" to their proper section when implemented, and add brief descriptions matching the style of existing entries.
