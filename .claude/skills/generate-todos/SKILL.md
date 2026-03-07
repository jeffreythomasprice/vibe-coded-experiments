---
name: generate-todos
description: Generate an implementation plan with numbered TODO steps from a feature description
disable-model-invocation: true
argument-hint: "<feature description> [--name <spec-file-name>]"
---

# Generate Implementation TODOs

Parse `$ARGUMENTS` to extract the feature description and an optional spec file name. If `--name <name>` is provided, use that as the spec file basename (without extension). Otherwise, generate a short kebab-case name summarizing the feature.

Example: `/generate-todos Add memory subsystem with 4096-cell linear memory --name memory-subsystem`
Example: `/generate-todos Implement projectile physics with lifetime tracking`

## Workflow

1. **Parse arguments.** Extract the feature description and optional `--name` flag. If no name is provided, derive a short kebab-case slug from the feature description (e.g. "projectile-physics").
2. **Read existing project context:**
   - Read `CLAUDE.md` for project conventions and architecture.
   - Read `design/implementation-plan.md` for the overall design, module layout, and existing phase structure.
   - Read `TODO.md` to understand what has already been done and what is planned.
   - Scan existing files in `design/specs/` to avoid duplicating work.
   - Read relevant source files to understand the current state of the codebase.
3. **Design the implementation plan.** Break the feature into numbered steps following these guidelines:
   - Steps should be ordered by dependency — earlier steps should not depend on later ones.
   - Each step should be small enough to implement in a single focused session.
   - Group related steps under phases if the feature is large enough to warrant it.
   - Each step should specify: what file(s) to create or modify, what structs/functions/traits to define, key behavior, and what tests to write.
   - Follow the level of detail seen in `design/implementation-plan.md` — include struct fields, method signatures, enum variants, and test expectations where applicable.
   - Reference existing code and types by name so the implementer knows exactly where to connect.
4. **Write the spec file** to `design/specs/<name>.md` with this structure:

   ```
   # <Feature Title>

   **Summary:** <1-2 sentence description>
   **Depends on:** <list any prerequisite phases/features, or "None">

   ---

   ## Steps

   ### <N.1> <Step title>

   **Files:** `path/to/file.rs`

   <Description of what to implement, including types, methods, behavior, and tests.>

   ### <N.2> <Step title>

   ...
   ```

5. **Generate a TODO list** in a new file `design/specs/<name>-todos.md`. Format each step as `- [ ] <N.X> <Step title>` so it can be consumed by `/implement-todo` and `/implement-all-todos`.
6. **Report** what was generated: the spec file path, the todos file path, the number of steps, and a brief summary of the plan.
