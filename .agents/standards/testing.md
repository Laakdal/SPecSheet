# Testing

## What to test

- **Every behaviour change gets a test** that fails without the change and passes with it.
- **Every bug fix starts with a test that reproduces the bug.** Watch it fail first; that proves the test catches the bug.
- **Test behaviour through public interfaces,** not private helpers or implementation details. A refactor should not break tests.
- **Cover the edges:** empty, missing, maximum, malformed and concurrent inputs where they apply, plus each error path you added.
- **Prefer real collaborators.** Mock only what is slow, non-deterministic or external (network, clock, payment providers). Never assert on mock behaviour alone.

## How tests should look

- One behaviour per test, with a name that states it: `rejects_expired_token`, not `test_auth_3`.
- Arrange, act, assert. Keep setup visible or in clearly named helpers.
- Tests are deterministic: no reliance on wall-clock time, ordering, network or shared state. Fix or quarantine a flaky test as soon as you see it, and say so.
- Use the project's existing framework, layout and fixtures. Don't introduce a second test framework.

## Levels

Use the cheapest test that gives confidence:
- **Unit** for logic.
- **Integration** for boundaries (database, HTTP, file system, queues).
- **End-to-end** for a few critical user paths.

Respect the project's existing split. If it has none, propose one in the plan rather than inventing it mid-task.

## Proving a change works

Before you say a task is done:

1. Run the test suite (or the relevant subset, plus the full suite before finishing), the linter or formatter, and the type-checker or build, using the commands in `AGENTS.md`.
2. Read the output, and quote the summary line (for example `42 passed, 0 failed`) in your report.
3. If something fails, report it, including failures you did not cause. A red test you didn't mention is a false report.
4. If you could not run something (missing tool, credentials, hardware), say exactly what was not verified.

**Never** delete, skip, loosen or rewrite a test just to make it pass. If a test is wrong, explain why and change it in its own step.
