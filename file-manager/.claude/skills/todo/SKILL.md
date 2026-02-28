---
description: Pick a task from TODO.md, plan it collaboratively, implement it, then clean up
argument-hint: task-description
---

Read @TODO.md to see the current task list.

**Step 1: Task Selection**

If "$ARGUMENTS" is provided and non-empty, find the task in TODO.md that best matches "$ARGUMENTS" and propose it as the selected task. Present it clearly to the user and confirm they want to proceed with it.

If no task was specified, analyze all tasks in TODO.md:
- Estimate the difficulty and scope of each (consider: files to change, research needed, architectural complexity, dependencies on other tasks)
- Sort from smallest/simplest to largest/most complex
- Present the ordered list with a brief rationale for each task's estimated scope

Use AskUserQuestion to confirm which task to tackle before proceeding. Do not begin any work until the user selects a task.

**Step 2: Planning**

Once a task is confirmed, use EnterPlanMode to:
1. Thoroughly explore the relevant parts of the codebase
2. Design a concrete, step-by-step implementation plan
3. Identify all files that will be created or modified
4. Note any architectural decisions or tradeoffs

Use AskUserQuestion if there are multiple valid approaches or requirements need clarification. Only exit plan mode and proceed when the user explicitly approves the plan.

**Step 3: Implementation**

Implement the approved plan. Stay within the agreed scope — do not add unrequested features or improvements.

**Step 4: Code Review**

Run the `code-review` skill to review the implementation before finalizing.

**Step 5: Cleanup**

After successful implementation:
1. Remove the completed task line from TODO.md
2. If the implementation introduced new patterns, architectural decisions, or key file locations that future sessions should know about, update CLAUDE.md to document them
