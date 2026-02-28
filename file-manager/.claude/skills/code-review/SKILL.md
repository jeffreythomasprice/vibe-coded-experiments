---
description: Review the current git diff for architecture violations, code style issues, and logical errors
---

Run the following bash command and capture the output:

```bash
git diff HEAD
```

If the diff is empty, also check staged changes:

```bash
git diff --cached
```

If both are empty, inform the user there is nothing to review.

Otherwise, review the diff carefully against these criteria from @CLAUDE.md:

**Architecture violations**
- CLI or web packages importing from `@file-manager/server` or `@file-manager/shared` (they should only use `@file-manager/schemas`)
- New storage backends implemented outside `packages/server/src/providers/`
- New backends not registered in `provider-registry.ts`
- Wire data (HTTP request/response) not going through `@file-manager/schemas` types
- Files fully buffered in memory when they should be streamed (`StorageProvider.read()` returns `AsyncIterable<Buffer>`; `write()` accepts `AsyncIterable<Buffer>`)
- Binary uploads not using `Content-Type: application/octet-stream`
- `Content-Type: application/json` set on requests with no body (causes Fastify 400)

**Code style violations**
- `.then()` / `.catch()` chains — must use `async`/`await` exclusively (ESLint rule)
- TypeScript strict-mode violations: unchecked indexed access, implicit `any`, missing return types where required, non-`override` on overrides
- `exactOptionalPropertyTypes` violations (assigning `undefined` to an optional field)
- `verbatimModuleSyntax` violations (value imports that should be `import type`)
- New files added without co-located tests when the change includes logic worth testing
- Helpers or abstractions created for a single use case (over-engineering)
- Error handling added for scenarios that can't happen within internal code paths

**Fastify-specific issues**
- Using `inject()` in tests instead of a real listening server on `port: 0`
- Missing or incorrect Fastify response schemas on new routes
- Route handlers that don't return the `reply` object (can cause response leaks)

**Logical errors**
- Path traversal not guarded by `LocalProvider.resolve()` before filesystem operations
- Off-by-one errors in path construction (double slashes, missing leading slash)
- Race conditions or missing `cancelled` flag in React `useEffect` cleanup
- State updates after component unmount
- Missing `await` on async calls that should be awaited
- Error objects caught but message not surfaced to the user

Present your findings as a structured report:

1. **Summary** — one sentence overview (e.g. "No issues found" or "3 issues found")
2. **Issues** — for each issue: severity (`error` / `warning` / `suggestion`), the file + line reference, and a clear explanation of the problem and how to fix it
3. If no issues are found, say so clearly and briefly

Be concise. Do not repeat back the diff contents verbatim. Do not suggest improvements outside the scope of what was changed.

After presenting the report, if there are any issues, ask two questions using the AskUserQuestion tool in a single call:

**Question 1** — `multiSelect: true`: Present each issue as a checkbox option. Label each option with a short description (severity + file + brief problem). Ask: "Which issues would you like to add to TODO.md?"

**Question 2** — `multiSelect: true`: Present the same list of issues again as checkbox options with the same labels. Ask: "Which issues would you like to fix right now?"

After the user responds:

1. For each issue selected in Question 1, append a new line to `TODO.md` in the following format:
   ```
   - [code-review] <severity>: <file:line> — <brief description>
   ```
   Use the Edit tool to append the selected items to the end of `TODO.md`. Confirm to the user which items were added.

2. For each issue selected in Question 2, use the EnterPlanMode tool to collaboratively plan and implement fixes. Handle them one at a time, in order of severity (errors first, then warnings, then suggestions).
