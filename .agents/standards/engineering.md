# Engineering

## Approaching a task

1. **Restate the goal** in one sentence, along with what "done" looks like. If you can't, the request is unclear (see the core block).
2. **Find the relevant code** before deciding anything. Read the entry points, the callers of what you'll change, and the existing tests.
3. **Size the work.**
   - A one-file change needs no plan.
   - A change spanning several files or modules needs a short plan (`plan-task` skill).
   - A change to architecture or a public interface needs a decision record (`review-architecture` skill).
4. **Work in small, verifiable steps.** Keep the code building and the tests passing after each step, so a failure points at the last step.
5. **Finish with evidence**, as described in [testing.md](testing.md#proving-a-change-works).

## Code quality

- Follow the conventions already in the codebase: naming, structure, formatting, error style. Consistency beats your personal preference.
- Prefer clear code to clever code. Names say what a thing is or does; a function does one job.
- Keep changes minimal: no drive-by renames, reformatting or "while I'm here" edits in the same change.
- Don't leave dead code, commented-out code, debug output, or a TODO without an owner or an issue link.
- Don't duplicate logic. Extract a shared helper the second time you need it, not before.
- Make invalid states hard to represent: use types, enums and validation at construction.
- Design for the current requirement. Add extension points only when a second real use exists.

## Errors and failure handling

- Fail loudly and early. Don't swallow exceptions, ignore return codes, or replace an error with a silent default.
- Validate input at system boundaries (user input, network, files, environment). Trust internal calls.
- Error messages name what failed, with which input, and what the caller can do about it. Never put secrets in them (see [security.md](security.md)).
- Handle a failure where you can actually do something about it. Otherwise let it propagate with context added.
- Clean up resources on every path: files, connections, locks, temp data.

## Refactoring

- A refactor changes structure, never behaviour. Have tests covering the behaviour **before** you start, and add characterization tests if they're missing.
- Keep refactors and behaviour changes in separate commits (ideally separate PRs).
- Move in steps that keep everything green. If a step breaks the tests, undo it rather than pushing on.

## Dependencies

- Prefer the standard library and what the project already uses. Every new dependency is code you now maintain and must trust.
- Before adding or upgrading one, run the `evaluate-dependency` skill: maintenance, license, security record, size, alternatives.
- Pin versions according to the project's lockfile conventions, and commit the lockfile change together with the code that needs it.

## Performance

- Measure before optimizing, and optimize the measured bottleneck.
- Avoid known traps: N+1 queries, unbounded loads into memory, blocking calls on hot or async paths, work inside loops that could be done once.
